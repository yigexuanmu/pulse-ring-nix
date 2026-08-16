//! Sonnet MG (motion-graphics) decorative background — faithful port of folia's
//! `sonnetShotMg*.ts`. [`MgCanvas`] records vector paths (lines / quadratic & cubic beziers /
//! arcs / circles / rects) and re-emits them every frame as [`CharQuad`]s with the same
//! entry-stagger growth folia's `AnimatedGraphics` uses. A camera transform is applied on the
//! CPU so line & triangle primitives stay correct under rotation/zoom (the shader can only
//! sample glyphs, pills, rects and triangles).

use crate::lyricview::{CharQuad, push_circle, push_line, push_triangle, SLOT_FRAME};

/// Stage-local → screen transform applied to every MG primitive before emission.
#[derive(Debug, Clone, Copy)]
pub struct MgXform {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
    pub tx: f32,
    pub ty: f32,
    pub rot: f32,
}

impl MgXform {
    pub fn point(&self, x: f32, y: f32) -> [f32; 2] {
        let rx = x * self.zoom;
        let ry = y * self.zoom;
        let cs = self.rot.cos();
        let sn = self.rot.sin();
        [
            self.cx + (rx * cs - ry * sn) + self.tx,
            self.cy + (rx * sn + ry * cs) + self.ty,
        ]
    }
}

/// One recorded path command. `len` is the command's arc length (for stroke growth).
#[derive(Debug, Clone, Copy)]
enum Cmd {
    Move { x: f32, y: f32 },
    Line { x: f32, y: f32, len: f32, last: [f32; 2] },
    Quad { c: [f32; 2], t: [f32; 2], len: f32, last: [f32; 2] },
    Cubic { c1: [f32; 2], c2: [f32; 2], t: [f32; 2], len: f32, last: [f32; 2] },
    Arc { c: [f32; 2], r: f32, start: f32, diff: f32, ccw: bool, len: f32, last: [f32; 2] },
    Circle { c: [f32; 2], r: f32, len: f32 },
    RectHint { x: f32, y: f32, w: f32, h: f32 },
}

impl Cmd {}

#[derive(Debug)]
struct Stroke {
    path: Vec<Cmd>,
    length: f32,
    color: [f32; 4],
    width: f32,
    delay: f32,
    span: f32,
}

#[derive(Debug)]
struct Fill {
    path: Vec<Cmd>,
    color: [f32; 4],
    alpha: f32,
    delay: f32,
    span: f32,
    rect_wipe: bool,
}

/// Vector drawing surface with folia's stagger schedule.
#[derive(Debug, Default)]
pub struct MgCanvas {
    current: Vec<Cmd>,
    current_len: f32,
    last: [f32; 2],
    strokes: Vec<Stroke>,
    fills: Vec<Fill>,
    scheduled: bool,
}

const GOLDEN: f32 = 0.6180339887498949;

impl MgCanvas {
    /// Number of completed stroke primitives recorded. Test-only mirror of
    /// PIXI `Graphics.geometry.graphicsData.length` — used by v2 scene shell
    /// ports as a coarse parity witness (not a byte-exact renderer frontier).
    pub fn strokes_count(&self) -> usize {
        self.strokes.len()
    }

    /// Number of completed fill primitives recorded. Test-only mirror of
    /// PIXI `Graphics.geometry.graphicsData.length` (fill sub-array).
    pub fn fills_count(&self) -> usize {
        self.fills.len()
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.current.push(Cmd::Move { x, y });
        self.last = [x, y];
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        let dx = x - self.last[0];
        let dy = y - self.last[1];
        let len = (dx * dx + dy * dy).sqrt();
        self.current.push(Cmd::Line { x, y, len, last: self.last });
        self.current_len += len;
        self.last = [x, y];
        self
    }

    pub fn quad_to(&mut self, cx: f32, cy: f32, tx: f32, ty: f32) -> &mut Self {
        let len = quad_len(self.last, [cx, cy], [tx, ty]);
        self.current.push(Cmd::Quad { c: [cx, cy], t: [tx, ty], len, last: self.last });
        self.current_len += len;
        self.last = [tx, ty];
        self
    }

    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, tx: f32, ty: f32) -> &mut Self {
        let len = cubic_len(self.last, [c1x, c1y], [c2x, c2y], [tx, ty]);
        self.current.push(Cmd::Cubic { c1: [c1x, c1y], c2: [c2x, c2y], t: [tx, ty], len, last: self.last });
        self.current_len += len;
        self.last = [tx, ty];
        self
    }

    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, start: f32, end: f32, ccw: bool) -> &mut Self {
        let mut diff = end - start;
        if ccw && diff > 0.0 {
            diff -= std::f32::consts::TAU;
        } else if !ccw && diff < 0.0 {
            diff += std::f32::consts::TAU;
        }
        let len = diff.abs() * r;
        self.current.push(Cmd::Arc {
            c: [cx, cy], r, start, diff, ccw, len, last: self.last,
        });
        self.current_len += len;
        let a = end;
        self.last = [cx + a.cos() * r, cy + a.sin() * r];
        self
    }

    pub fn circle(&mut self, x: f32, y: f32, r: f32) -> &mut Self {
        let len = std::f32::consts::TAU * r;
        self.current.push(Cmd::Circle { c: [x, y], r, len });
        self.current_len += len;
        self.last = [x + r, y];
        self
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.current.push(Cmd::RectHint { x, y, w, h });
        self.move_to(x, y)
            .line_to(x + w, y)
            .line_to(x + w, y + h)
            .line_to(x, y + h)
            .line_to(x, y);
        self
    }

    /// `drawEllipse(x, y, rx, ry)` — mirrors PixiJS `Graphics.drawEllipse`, which
    /// internally converts the ellipse into four cubic-bezier curves using the
    /// standard ellipse-to-bezier approximation (kappa ≈ 0.5522847498307796).
    /// `cx`,`cy` is the centre; `rx`,`ry` are the half-width and half-height.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> &mut Self {
        let kappa = 0.5522847498307796_f32;
        let ox = rx * kappa; // horizontal offset of control points
        let oy = ry * kappa; // vertical offset of control points
        let right_x = cx + rx;
        let left_x = cx - rx;
        let top_y = cy - ry;
        let bottom_y = cy + ry;
        // Start at the rightmost point and trace the ellipse counter-clockwise
        // via four cubic-bezier quarter-arcs (identical to PixiJS).
        self.move_to(right_x, cy);
        self.cubic_to(right_x, cy - oy, cx + ox, top_y, cx, top_y);
        self.cubic_to(cx - ox, top_y, left_x, cy - oy, left_x, cy);
        self.cubic_to(left_x, cy + oy, cx - ox, bottom_y, cx, bottom_y);
        self.cubic_to(cx + ox, bottom_y, right_x, cy + oy, right_x, cy);
        self
    }

    pub fn stroke(&mut self, color: [f32; 4], width: f32, alpha: f32) -> &mut Self {
        if !self.current.is_empty() {
            let mut color = color;
            color[3] *= alpha;
            self.strokes.push(Stroke {
                path: std::mem::take(&mut self.current),
                length: self.current_len,
                color,
                width,
                delay: 0.0,
                span: 0.0,
            });
            self.current_len = 0.0;
        }
        self
    }

    pub fn fill(&mut self, color: [f32; 4], alpha: f32) -> &mut Self {
        if !self.current.is_empty() {
            let rect_wipe = self.current.len() == 6 && matches!(self.current[0], Cmd::RectHint { .. });
            self.fills.push(Fill {
                path: std::mem::take(&mut self.current),
                color,
                alpha,
                delay: 0.0,
                span: 0.0,
                rect_wipe,
            });
            self.current_len = 0.0;
        }
        self
    }

    /// Golden-ratio stagger slots (pure function of command order — seek-safe).
    fn schedule(&mut self) {
        if self.scheduled {
            return;
        }
        let mut s = 0usize;
        let mut f = 0usize;
        for cmd in &mut self.strokes {
            let idx = s;
            s += 1;
            let slot = (idx as f32 * GOLDEN) % 1.0;
            let jitter = ((idx as u64).wrapping_mul(2654435761) >> 32) as f32 / u32::MAX as f32;
            let delay = slot * 0.5;
            let span = (0.32 + jitter * 0.26).min(1.0 - delay);
            cmd.delay = delay;
            cmd.span = span;
        }
        for cmd in &mut self.fills {
            let idx = f;
            f += 1;
            let slot = (idx as f32 * GOLDEN) % 1.0;
            let jitter = ((idx as u64).wrapping_mul(2654435761) >> 32) as f32 / u32::MAX as f32;
            let delay = slot * 0.45;
            let span = (0.4 + jitter * 0.25).min(1.0 - delay);
            cmd.delay = delay;
            cmd.span = span;
        }
        self.scheduled = true;
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.fills.is_empty()
    }

    /// Emit the canvas at `raw_progress` (0..1) onto `out`, applying `xf` to every point.
    pub fn emit(&mut self, raw_progress: f32, xf: &MgXform, out: &mut Vec<CharQuad>) {
        if self.is_empty() {
            return;
        }
        self.schedule();
        let p = raw_progress.clamp(0.0, 1.0);
        for s in &self.strokes {
            let local_raw = ((p - s.delay) / s.span.max(0.001)).clamp(0.0, 1.0);
            let local = 1.0 - (1.0 - local_raw).powi(3);
            emit_stroke(s, local, xf, out);
        }
        for f in &self.fills {
            let local_raw = ((p - f.delay) / f.span.max(0.001)).clamp(0.0, 1.0);
            emit_fill(f, local_raw, xf, out);
        }
    }
}

// ------------------------------------------------------------------ tessellation

fn quad_len(p0: [f32; 2], c: [f32; 2], t: [f32; 2]) -> f32 {
    let mut prev = p0;
    let mut len = 0.0;
    let n = 6;
    for i in 1..=n {
        let tt = i as f32 / n as f32;
        let pt = quad_point(p0, c, t, tt);
        len += (pt[0] - prev[0]).hypot(pt[1] - prev[1]);
        prev = pt;
    }
    len
}

fn cubic_len(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], t: [f32; 2]) -> f32 {
    let mut prev = p0;
    let mut len = 0.0;
    let n = 10;
    for i in 1..=n {
        let tt = i as f32 / n as f32;
        let pt = cubic_point(p0, c1, c2, t, tt);
        len += (pt[0] - prev[0]).hypot(pt[1] - prev[1]);
        prev = pt;
    }
    len
}

fn quad_point(p0: [f32; 2], c: [f32; 2], t: [f32; 2], tt: f32) -> [f32; 2] {
    let mt = 1.0 - tt;
    [
        mt * mt * p0[0] + 2.0 * mt * tt * c[0] + tt * tt * t[0],
        mt * mt * p0[1] + 2.0 * mt * tt * c[1] + tt * tt * t[1],
    ]
}

fn cubic_point(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], t: [f32; 2], tt: f32) -> [f32; 2] {
    let mt = 1.0 - tt;
    [
        mt * mt * mt * p0[0]
            + 3.0 * mt * mt * tt * c1[0]
            + 3.0 * mt * tt * tt * c2[0]
            + tt * tt * tt * t[0],
        mt * mt * mt * p0[1]
            + 3.0 * mt * mt * tt * c1[1]
            + 3.0 * mt * tt * tt * c2[1]
            + tt * tt * tt * t[1],
    ]
}

/// Polyline of the command (points on the curve, step-count depends on the curve type).
fn tessellate(cmd: &Cmd, last: [f32; 2]) -> Vec<[f32; 2]> {
    match *cmd {
        Cmd::Move { .. } => vec![],
        Cmd::Line { x, y, .. } => vec![[x, y]],
        Cmd::Quad { c, t, .. } => {
            let n = 6;
            (1..=n).map(|i| quad_point(last, c, t, i as f32 / n as f32)).collect()
        }
        Cmd::Cubic { c1, c2, t, .. } => {
            let n = 10;
            (1..=n).map(|i| cubic_point(last, c1, c2, t, i as f32 / n as f32)).collect()
        }
        Cmd::Arc { c, r, start, diff, ccw, .. } => {
            let n = ((diff.abs() * r) / 30.0).ceil() as usize + 2;
            let n = n.max(5).min(28);
            let mut pts = Vec::with_capacity(n);
            for i in 1..=n {
                let a = start + diff * (i as f32 / n as f32);
                let _ = ccw;
                pts.push([c[0] + a.cos() * r, c[1] + a.sin() * r]);
            }
            pts
        }
        Cmd::Circle { c, r, .. } => {
            let n = 12;
            let mut pts = Vec::with_capacity(n);
            for i in 1..=n {
                let a = std::f32::consts::TAU * (i as f32 / n as f32);
                pts.push([c[0] + a.cos() * r, c[1] + a.sin() * r]);
            }
            pts
        }
        Cmd::RectHint { .. } => vec![],
    }
}

// ------------------------------------------------------------------ emission

fn emit_line(out: &mut Vec<CharQuad>, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4], xf: &MgXform) {
    let pa = xf.point(a[0], a[1]);
    let pb = xf.point(b[0], b[1]);
    push_line(out, pa[0], pa[1], pb[0], pb[1], (width * xf.zoom).max(0.5), color[3], color);
}

fn emit_stroke(s: &Stroke, local: f32, xf: &MgXform, out: &mut Vec<CharQuad>) {
    if s.length <= 0.0 || local <= 0.0 {
        return;
    }
    let target = s.length * local;
    let mut remaining = target;
    let mut current = [0.0f32, 0.0];
    for cmd in &s.path {
        match *cmd {
            Cmd::Move { x, y } => {
                current = [x, y];
            }
            _ => {
                if remaining <= 0.0 {
                    break;
                }
                let pts = tessellate(cmd, current);
                for b in pts {
                    let seg = (b[0] - current[0]).hypot(b[1] - current[1]);
                    if seg <= 0.0 {
                        continue;
                    }
                    if remaining >= seg {
                        emit_line(out, current, b, s.width, s.color, xf);
                        current = b;
                        remaining -= seg;
                    } else {
                        let r = remaining / seg;
                        let bx = current[0] + (b[0] - current[0]) * r;
                        let by = current[1] + (b[1] - current[1]) * r;
                        emit_line(out, current, [bx, by], s.width, s.color, xf);
                        remaining = 0.0;
                        break;
                    }
                }
            }
        }
    }
}

/// Simple ear-clipping triangulation (convex & mildly concave polygons).
fn triangulate(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let mut tris = Vec::with_capacity(n - 2);
    let mut idx: Vec<usize> = (0..n).collect();
    let mut guard = 0;
    while idx.len() > 3 {
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let a = idx[i];
            let b = idx[(i + 1) % m];
            let c = idx[(i + 2) % m];
            if cross_2d(points[a], points[b], points[c]) >= 0.0 {
                // convex corner; check it's an ear (no other point inside)
                let mut ok = true;
                for &j in &idx {
                    if j == a || j == b || j == c {
                        continue;
                    }
                    if point_in_tri(points[j], points[a], points[b], points[c]) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    tris.push([a, b, c]);
                    idx.remove((i + 1) % m);
                    clipped = true;
                    break;
                }
            }
        }
        if !clipped {
            // fallback: fan from first vertex
            for i in 1..idx.len() - 1 {
                tris.push([idx[0], idx[i], idx[i + 1]]);
            }
            break;
        }
    }
    if idx.len() == 3 {
        tris.push([idx[0], idx[1], idx[2]]);
    }
    tris
}

fn cross_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_tri(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let d1 = cross_2d(p, a, b);
    let d2 = cross_2d(p, b, c);
    let d3 = cross_2d(p, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn emit_fill(f: &Fill, local_raw: f32, xf: &MgXform, out: &mut Vec<CharQuad>) {
    let local = 1.0 - (1.0 - local_raw.clamp(0.0, 1.0)).powi(3);
    let alpha_progress = 1.0 - (1.0 - (local_raw.clamp(0.0, 1.0) * 2.0).min(1.0)).powi(3);
    let alpha = f.alpha * alpha_progress;
    if alpha <= 0.004 || local <= 0.0 {
        return;
    }
    if f.rect_wipe {
        // left→right wipe: animate the rect width
        let Cmd::RectHint { x, y, w, h } = f.path[0] else {
            return;
        };
        let w = w * local;
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let pc = xf.point(cx, cy);
        let mut color = f.color;
        color[3] = alpha;
        out.push(CharQuad {
            glow: SLOT_FRAME,
            uv: [0.0; 4],
            px: [(w * xf.zoom).max(0.1), (h * xf.zoom).max(0.1)],
            pos: pc,
            scale: 1.0,
            alpha,
            rotate: xf.rot,
            color,
            ext: [0.0; 4],
        });
        return;
    }
    // Single full circle fill → one pill quad (exact circle), keeps halftone-style scenes cheap.
    if f.path.len() == 1 {
        if let Cmd::Circle { c, r, .. } = f.path[0] {
            let pc = xf.point(c[0], c[1]);
            let mut color = f.color;
            color[3] = alpha;
            push_circle(out, pc[0], pc[1], (r * xf.zoom).max(0.1), alpha, color);
            return;
        }
    }
    // Polygon fill: collect closed outline points (tessellate curves).
    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut current = [0.0f32, 0.0];
    let mut started = false;
    for cmd in &f.path {
        match *cmd {
            Cmd::Move { x, y } => {
                if started && points.len() >= 3 {
                    break;
                }
                current = [x, y];
                started = true;
            }
            Cmd::RectHint { .. } => {}
            _ => {
                let pts = tessellate(cmd, current);
                points.extend(pts.iter().copied());
                if let Some(p) = pts.last() {
                    current = *p;
                }
            }
        }
    }
    if points.len() < 3 {
        return;
    }
    let mut color = f.color;
    color[3] = alpha;
    emit_polygon(out, &points, alpha, color, xf);
}

/// Triangulate `points` and emit filled triangles (public so particle stars can reuse it).
pub fn emit_polygon(out: &mut Vec<CharQuad>, points: &[[f32; 2]], alpha: f32, color: [f32; 4], xf: &MgXform) {
    if points.len() < 3 {
        return;
    }
    for tri in triangulate(points) {
        let a = xf.point(points[tri[0]][0], points[tri[0]][1]);
        let b = xf.point(points[tri[1]][0], points[tri[1]][1]);
        let c = xf.point(points[tri[2]][0], points[tri[2]][1]);
        push_triangle(out, a, b, c, alpha, color);
    }
}

// ------------------------------------------------------------------ shared helpers

/// `fillPolygon` / `strokePolygon` from `sonnetThemedShotMgPrimitives.ts`.
pub fn fill_polygon(t: &mut MgCanvas, points: &[[f32; 2]], color: [f32; 4], alpha: f32) {
    t.move_to(points[0][0], points[0][1]);
    for p in &points[1..] {
        t.line_to(p[0], p[1]);
    }
    t.line_to(points[0][0], points[0][1]);
    t.fill(color, alpha);
}

pub fn stroke_polygon(t: &mut MgCanvas, points: &[[f32; 2]], color: [f32; 4], alpha: f32, width: f32) {
    t.move_to(points[0][0], points[0][1]);
    for p in &points[1..] {
        t.line_to(p[0], p[1]);
    }
    t.line_to(points[0][0], points[0][1]);
    t.stroke(color, width, alpha);
}

/// `drawLeaf` / `drawPetal` from `sonnetThemedShotMgPrimitives.ts`.
pub fn draw_leaf(t: &mut MgCanvas, x: f32, y: f32, length: f32, width: f32, angle: f32, color: [f32; 4], fill_alpha: f32) {
    let dx = angle.cos();
    let dy = angle.sin();
    let nx = -dy;
    let ny = dx;
    let tip_x = x + dx * length;
    let tip_y = y + dy * length;
    t.move_to(x, y)
        .quad_to(x + dx * length * 0.45 + nx * width, y + dy * length * 0.45 + ny * width, tip_x, tip_y)
        .quad_to(x + dx * length * 0.45 - nx * width, y + dy * length * 0.45 - ny * width, x, y)
        .fill(color, fill_alpha);
    t.move_to(x, y)
        .quad_to(x + dx * length * 0.45 + nx * width, y + dy * length * 0.45 + ny * width, tip_x, tip_y)
        .quad_to(x + dx * length * 0.45 - nx * width, y + dy * length * 0.45 - ny * width, x, y)
        .stroke(color, 1.5, (fill_alpha * 3.2).min(0.8));
    t.move_to(x, y).line_to(tip_x, tip_y).stroke(color, 1.0, 0.32);
}

pub fn draw_petal(t: &mut MgCanvas, cx: f32, cy: f32, length: f32, width: f32, angle: f32, color: [f32; 4], fill_alpha: f32) {
    draw_leaf(t, cx, cy, length, width, angle, color, fill_alpha);
}

/// Overscan extents so open MG paths continue beyond the viewport (`sonnetShotMgViewport.ts`).
pub fn shot_mg_bleed(width: f32, height: f32, radius: f32) -> [f32; 2] {
    [
        (radius * 0.92).max(width * 0.64),
        (radius * 0.92).max(height * 0.64),
    ]
}
