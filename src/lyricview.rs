//! Shared lyric rendering core: style-agnostic primitives that every lyric style builds on.
//!
//! A lyric style (sonnet, ...) receives a [`StyleCtx`] + [`StyleInput`] and returns a
//! list of [`CharQuad`]s — one quad per character. Quads sample the SDF glyph atlas in the
//! shader, so text is crisp at any zoom/rotation and supports glow. Adding a new animation
//! style = one new module + one enum arm (see [`build_frame`]).

use crate::lyrics::LyricLine;
use crate::sdf::{GlyphAtlas, PAD, RASTER_PX};

/// One animated character quad, matching the WGSL `lyric_words` array (20 f32 each).
///
/// The first field doubles as a "slot" selector:
/// - `0..=1`  → SDF glyph quad (`uv` = glyph UV rect, `glow` carries glow intensity).
/// - `252`    → filled triangle (MG decoration). Vertices are in stage-local px relative to
///              the quad centre: `ext[0..1]=v0`, `ext[2..3]=v1`, `uv[0..1]=v2`, `uv[2..3]=0`.
///              `px` is the bounding-box size, `pos` the bbox centre, `scale=1`, `rotate=0`.
/// - `254`    → low-corner-radius filled rect (frame bars, line segments — set `rotate` to
///              the segment angle and `px=[length, thickness]` to draw a line).
/// - `255`    → fully-rounded pill (a `lw==lh` pill is an exact circle fill).
#[derive(Debug, Clone, Copy)]
pub struct CharQuad {
    /// Glow intensity 0..1 for glyph quads, or a slot sentinel (see type docs).
    pub glow: f32,
    /// Glyph UV rect, or triangle vertex data (see type docs).
    pub uv: [f32; 4],
    /// Quad size in screen px (cell size × glyph scale).
    pub px: [f32; 2],
    /// Centre position in screen px.
    pub pos: [f32; 2],
    pub scale: f32,
    pub alpha: f32,
    pub rotate: f32,
    pub color: [f32; 4],
    /// Extra channel used by shape slots (triangle vertices v0/v1).
    pub ext: [f32; 4],
}

impl CharQuad {
    pub fn to_array(&self) -> [f32; 20] {
        // Field order is perf-sensitive: the WGSL `scene_at` lyric loop reads the
        // AABB + transform fields (glow/slot, px, pos, scale, alpha) at offsets 0..6
        // FIRST so it can reject out-of-view / invisible quads before touching the
        // survivor-only fields (uv, rotate, color, ext). Keep this paired with the
        // indices in src/draw.rs `scene_at` and `set_lyrics` — they must agree or the
        // shader reads garbage.
        [
            self.glow,
            self.px[0], self.px[1],
            self.pos[0], self.pos[1],
            self.scale,
            self.alpha,
            self.uv[0], self.uv[1], self.uv[2], self.uv[3],
            self.rotate,
            self.color[0], self.color[1], self.color[2], self.color[3],
            self.ext[0], self.ext[1], self.ext[2], self.ext[3],
        ]
    }
}

/// Sentinel `glow` value: draw a rounded-rect pill background instead of a glyph quad
/// (used for the translation subtitle bar; a square pill is an exact circle).
pub const SLOT_PILL: f32 = 255.0;
/// Sentinel: draw a low-corner-radius filled rect (frame decor bars / line segments).
pub const SLOT_FRAME: f32 = 254.0;
/// Sentinel: draw a filled triangle (MG decorations). Vertex data in `ext`/`uv`.
pub const SLOT_TRI: f32 = 252.0;

/// Post-processing values a style wants applied to the whole lyric layer this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct LyricFx {
    /// Blur strength (0..1): softens glyph edges (transition "fast-blur").
    pub blur: f32,
    /// Glitch strength (0..1): quantised horizontal slice displacement (transition "glitch").
    pub glitch: f32,
    /// Film-grain noise (0..1) added to the lyric alpha.
    pub noise: f32,
    /// Contrast (0..1): pushes lyric colours toward pure black/white.
    pub contrast: f32,
    /// Global glow boost on top of per-word glow.
    pub glow: f32,
    /// Chromatic aberration (0..1): R/B glyphs offset slightly for a lens edge.
    pub chromatic: f32,
    /// Full-screen RGB shift (0..1): R/B channels sampled at ±1.25px on the 25° axis.
    pub rgb_shift: f32,
    /// Halftone dot screen amount (0..1).
    pub halftone: f32,
    /// Full-scene vignette amount (0..1).
    pub vignette: f32,
    /// folia postProcessLensDistortion: full-frame radial barrel curvature (0..2).
    pub lens_distortion: f32,
}

impl LyricFx {
    pub fn to_array(&self) -> [f32; 9] {
        // fx[8] = lens_distortion is appended so the existing WGSL u.lyric_fx[0..8] indices
        // stay valid; the new WGSL barrel warp (`scene_at`) reads fx[8] for the curvature.
        [self.blur, self.glitch, self.noise, self.contrast, self.chromatic, self.rgb_shift, self.halftone, self.vignette, self.lens_distortion]
    }
}

/// Result of a style's frame build: the character quads plus the fx values.
pub struct StyleOutput {
    pub quads: Vec<CharQuad>,
    pub fx: LyricFx,
}

impl StyleOutput {
    pub fn empty() -> Self {
        Self { quads: Vec::new(), fx: LyricFx::default() }
    }
}

/// Colour palette shared by all lyric styles.
#[derive(Debug, Clone, Copy)]
pub struct LyricColors {
    pub primary: [f32; 4],
    pub accent: [f32; 4],
    pub dim: [f32; 4],
    /// Secondary accent used by the sonnet MG decoration layer.
    pub secondary: [f32; 4],
    /// Translucent pill background for the subtitle bar.
    pub pill: [f32; 4],
}

impl Default for LyricColors {
    fn default() -> Self {
        Self {
            primary: [1.0, 1.0, 1.0, 0.92],
            accent: [0.85, 0.72, 1.0, 1.0],
            dim: [1.0, 1.0, 1.0, 0.72],
            secondary: [0.62, 0.66, 0.88, 1.0],
            pill: [0.0, 0.0, 0.0, 0.45],
        }
    }
}

/// Timing context for one lyric line during animation (seconds).
#[derive(Debug, Clone, Copy)]
pub struct LineTiming {
    pub start: f32,
    pub end: f32,
    pub duration: f32,
}

/// Per-word timing + grouping derived from a line (shared by all styles).
#[derive(Debug, Clone)]
pub struct WordBound {
    pub text: String,
    pub is_cjk: bool,
    /// Total chars in the line up to this word.
    pub char_offset: u32,
    pub start: f32,
    pub end: f32,
}

/// Detect CJK text (Han / Hiragana / Katakana / Hangul).
pub fn is_cjk(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}'))
}

fn is_cjk_char(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
}

/// Split a line into display words. The key rule for CJK is that **consecutive CJK
/// characters stay grouped as one word** — per-character fly-in is handled inside
/// `push_word_full` by iterating the word's `chars()`. Splitting "你好世界" into four
/// single-character words caused the giant-decoration pass to copy the *hero character*
/// (e.g. "好") as a background echo at 1.4-2.2× — the user saw the same glyph rendered
/// twice simultaneously. Latin/digit runs group into words; punctuation sticks to the
/// preceding word (folia's sticky segments).
pub fn segment_words(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    for token in line.split_whitespace() {
        for ch in token.chars() {
            if is_cjk_char(ch) {
                match &mut cur {
                    // Extend a CJK run that was already started this token.
                    Some(w) if w.chars().next().map(is_cjk_char).unwrap_or(false) => {
                        w.push(ch);
                    }
                    _ => {
                        if let Some(w) = cur.take() {
                            out.push(w);
                        }
                        cur = Some(ch.to_string());
                    }
                }
            } else if ch.is_alphanumeric() {
                let mut w = cur.take().unwrap_or_default();
                w.push(ch);
                cur = Some(w);
            } else if ch.is_whitespace() {
                if let Some(w) = cur.take() {
                    out.push(w);
                }
            } else {
                // Punctuation / symbols: stick to the current word or the last pushed one.
                match &mut cur {
                    Some(w) => w.push(ch),
                    None => {
                        if let Some(last) = out.last_mut() {
                            last.push(ch);
                        } else {
                            out.push(ch.to_string());
                        }
                    }
                }
            }
        }
        if let Some(w) = cur.take() {
            out.push(w);
        }
    }
    out
}

/// Words with per-word timing. Uses the adapter's per-char timestamps when they match the
/// line's char count (accurate per-char reveal), otherwise distributes over the duration.
pub fn split_with_timing(line: &LyricLine, timing: &LineTiming) -> Vec<WordBound> {
    let words = segment_words(&line.text);
    let total_chars: u32 = words.iter().map(|w| w.chars().count() as u32).sum();
    let use_chars = !line.chars.is_empty() && line.chars.len() as u32 == total_chars;
    let mut out = Vec::with_capacity(words.len());
    let mut offset: u32 = 0;
    for w in words {
        let count = w.chars().count() as u32;
        let (start, end) = if use_chars && count > 0 {
            let s = line.chars[offset as usize] as f32 / 1000.0;
            let e_idx = (offset + count - 1).min(line.chars.len() as u32 - 1) as usize;
            let e = line.chars[e_idx] as f32 / 1000.0;
            (s.max(timing.start), (e + 0.12).min(timing.end))
        } else {
            let fs = offset as f32 / total_chars.max(1) as f32;
            let fe = (offset + count) as f32 / total_chars.max(1) as f32;
            (timing.start + timing.duration * fs, timing.start + timing.duration * fe)
        };
        out.push(WordBound {
            is_cjk: is_cjk(&w),
            char_offset: offset,
            text: w,
            start,
            end,
        });
        offset += count;
    }
    out
}

/// Measure the total advance width of `text` at `size_px`.
pub fn measure_text(atlas: &GlyphAtlas, text: &str, size_px: f32) -> f32 {
    atlas.measure(text, size_px, 0)
}

/// Measure with the bold font (weight 1).
pub fn measure_text_bold(atlas: &GlyphAtlas, text: &str, size_px: f32) -> f32 {
    atlas.measure(text, size_px, 1)
}

/// Emit a quad for every character of `word` at the given pen/baseline, returning the width.
///
/// `glyph_scale = size_px / RASTER_PX`. Per-char quads inherit the caller's per-word params so
/// word-level animation (alpha/scale/rotate/glow) applies uniformly to its characters.
///
/// When `char_enter` is `Some`, each character additionally gets its own entry offset
/// `(dx, dy, rot)` scaled by `enter_amount` (0..1) — used by sonnet for folia-style per-glyph
/// staggered fly-in ("回正"). Each entry of `char_enter` matches one visible character.
#[allow(clippy::too_many_arguments)]
pub fn push_word(
    atlas: &GlyphAtlas,
    out: &mut Vec<CharQuad>,
    word: &str,
    pen_x: f32,
    baseline_y: f32,
    size_px: f32,
    weight: u8,
    alpha: f32,
    scale: f32,
    rotate: f32,
    offset: [f32; 2],
    color: [f32; 4],
    glow: f32,
) -> f32 {
    push_word_enter(atlas, out, word, pen_x, baseline_y, size_px, weight, alpha, scale, rotate, offset, color, glow, None, 0.0, 0.0)
}

/// `push_word` with per-character entry offsets (folia-style staggered glyph fly-in) and a
/// per-word chromatic-aberration amount (packed into the quad's `ext[0]` for the shader).
/// `ghost` = (dx, dy, alpha): emits a hollow after-image of every character offset by `dx,dy`
/// (folia semi-hero echo ghosts).
#[allow(clippy::too_many_arguments)]
pub fn push_word_enter(
    atlas: &GlyphAtlas,
    out: &mut Vec<CharQuad>,
    word: &str,
    pen_x: f32,
    baseline_y: f32,
    size_px: f32,
    weight: u8,
    alpha: f32,
    scale: f32,
    rotate: f32,
    offset: [f32; 2],
    color: [f32; 4],
    glow: f32,
    char_enter: Option<&[(f32, f32, f32)]>,
    enter_amount: f32,
    ca_amount: f32,
) -> f32 {
    push_word_full(atlas, out, word, pen_x, baseline_y, size_px, weight, alpha, scale, rotate, offset, color, glow, char_enter, enter_amount, ca_amount, None, false, None, 0.0, None)
}

/// Like `push_word_enter` plus per-character ghost echoes `(dx, dy, alpha)` and vertical
/// CJK column stacking (each visible char advances `size*0.9` down the column).
#[allow(clippy::too_many_arguments)]
pub fn push_word_full(
    atlas: &GlyphAtlas,
    out: &mut Vec<CharQuad>,
    word: &str,
    pen_x: f32,
    baseline_y: f32,
    size_px: f32,
    weight: u8,
    alpha: f32,
    scale: f32,
    rotate: f32,
    offset: [f32; 2],
    color: [f32; 4],
    glow: f32,
    char_enter: Option<&[(f32, f32, f32)]>,
    enter_amount: f32,
    ca_amount: f32,
    ghost: Option<(f32, f32, f32)>,
    vertical: bool,
    // Per-character entry progress 0..1 (folia per-grapheme timing). When `Some`, each
    // character enters on its own clock: offsets/CA scale with (1-fly_i) and the quad alpha
    // follows core_i = 0.16 + fly_i*0.84, normalised by the word-level `core`.
    char_fly: Option<&[f32]>,
    core: f32,
    // Per-character settle scale (folia: 0.86 + fly_i*0.14, type-impact emphasis
    // 0.52 + fly_i*0.48). Multiplied into the quad scale while the glyph 回正s.
    char_scale: Option<&[f32]>,
) -> f32 {
    if alpha <= 0.004 || word.is_empty() {
        return atlas.measure(word, size_px, weight);
    }
    let s = size_px / RASTER_PX;
    let cell_px = crate::sdf::CELL as f32 * s;
    let placed = atlas.layout(word, size_px, weight);
    let mut width = 0.0f32;
    let mut ci = 0usize;
    for p in &placed {
        let Some(info) = atlas.glyph(p.ch, weight) else {
            width += p.advance;
            ci += 1;
            continue;
        };
        let mut dx = offset[0];
        let mut dy = offset[1];
        let mut rot = 0.0f32;
        let mut ca = ca_amount;
        let mut quad_scale = scale;
        let mut quad_alpha = alpha;
        if let Some(ce) = char_enter {
            if let Some(&(ex, ey, er)) = ce.get(ci) {
                let amount = char_fly.map(|cf| 1.0 - cf.get(ci).copied().unwrap_or(1.0)).unwrap_or(enter_amount);
                dx += ex * amount;
                dy += ey * amount;
                rot = er * amount;
            }
        }
        if let Some(cf) = char_fly {
            if let Some(&fly_i) = cf.get(ci) {
                let core_i = 0.16 + fly_i * 0.84;
                quad_alpha = alpha * core_i / core.max(0.001);
                ca = ca * (1.0 - fly_i);
            }
        }
        if let Some(cs) = char_scale {
            if let Some(&scale_i) = cs.get(ci) {
                quad_scale = quad_scale * scale_i;
            }
        }
        // Vertical CJK column: characters stack downward, centred on the column axis.
        let (qx0, qy0) = if vertical {
            (
                pen_x + dx,
                baseline_y + ci as f32 * size_px * 0.9 + (info.ymin - PAD as f32) * s + dy,
            )
        } else {
            (
                pen_x + p.start + (info.xmin - PAD as f32) * s + dx,
                baseline_y + (info.ymin - PAD as f32) * s + dy,
            )
        };
        out.push(CharQuad {
            glow,
            uv: info.uv,
            px: [cell_px, cell_px],
            pos: [qx0 + cell_px * 0.5, qy0 + cell_px * 0.5],
            scale: quad_scale,
            alpha: quad_alpha,
            rotate: rotate + rot,
            color,
            ext: [ca, 0.0, 0.0, 0.0],
        });
        // Ghost echo: hollow after-image offset along the layout normal, low alpha.
        if let Some((gx, gy, ga)) = ghost {
            if ga > 0.004 {
                let mut gc = color;
                gc[3] = ga;
                out.push(CharQuad {
                    glow: 0.0,
                    uv: info.uv,
                    px: [cell_px, cell_px],
                    pos: [qx0 + cell_px * 0.5 + gx, qy0 + cell_px * 0.5 + gy],
                    scale: scale * 0.98,
                    alpha: quad_alpha * (ga / alpha.max(0.001)).min(1.0),
                    rotate: rotate + rot,
                    color: gc,
                    ext: [0.0, 0.0, 0.0, 0.0],
                });
            }
        }
        width += p.advance;
        ci += 1;
    }
    width
}

/// Push a rounded-rect pill quad (translation subtitle bar background).
pub fn push_pill(
    out: &mut Vec<CharQuad>,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    alpha: f32,
    color: [f32; 4],
) {
    if alpha <= 0.004 {
        return;
    }
    out.push(CharQuad {
        glow: SLOT_PILL,
        uv: [0.0; 4],
        px: [w, h],
        pos: [cx, cy],
        scale: 1.0,
        alpha,
        rotate: 0.0,
        color,
        ext: [0.0; 4],
    });
}

/// Push a low-corner-radius filled rect (frame decor bars / ornaments).
pub fn push_rect(
    out: &mut Vec<CharQuad>,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    alpha: f32,
    color: [f32; 4],
) {
    if alpha <= 0.004 {
        return;
    }
    out.push(CharQuad {
        glow: SLOT_FRAME,
        uv: [0.0; 4],
        px: [w, h],
        pos: [cx, cy],
        scale: 1.0,
        alpha,
        rotate: 0.0,
        color,
        ext: [0.0; 4],
    });
}

/// Push a line segment from `(x0,y0)` to `(x1,y1)` with the given thickness, drawn as a
/// rotated thin rect. Matches folia's default butt-cap `stroke`.
pub fn push_line(
    out: &mut Vec<CharQuad>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    alpha: f32,
    color: [f32; 4],
) {
    if alpha <= 0.004 {
        return;
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0001 {
        return;
    }
    out.push(CharQuad {
        glow: SLOT_FRAME,
        uv: [0.0; 4],
        px: [len, thickness],
        pos: [(x0 + x1) * 0.5, (y0 + y1) * 0.5],
        scale: 1.0,
        alpha,
        rotate: dy.atan2(dx),
        color,
        ext: [0.0; 4],
    });
}

/// Push a filled circle (`lw==lh` pill quad). `color` is RGBA, `alpha` an extra multiplier.
pub fn push_circle(
    out: &mut Vec<CharQuad>,
    cx: f32,
    cy: f32,
    r: f32,
    alpha: f32,
    color: [f32; 4],
) {
    if alpha <= 0.004 || r <= 0.0 {
        return;
    }
    out.push(CharQuad {
        glow: SLOT_PILL,
        uv: [0.0; 4],
        px: [r * 2.0, r * 2.0],
        pos: [cx, cy],
        scale: 1.0,
        alpha,
        rotate: 0.0,
        color,
        ext: [0.0; 4],
    });
}

/// Push a filled triangle. Vertices are in stage-local px; the quad is centred on the
/// triangle's bounding box so the shader can render it directly.
pub fn push_triangle(
    out: &mut Vec<CharQuad>,
    v0: [f32; 2],
    v1: [f32; 2],
    v2: [f32; 2],
    alpha: f32,
    color: [f32; 4],
) {
    if alpha <= 0.004 {
        return;
    }
    let min_x = v0[0].min(v1[0]).min(v2[0]);
    let max_x = v0[0].max(v1[0]).max(v2[0]);
    let min_y = v0[1].min(v1[1]).min(v2[1]);
    let max_y = v0[1].max(v1[1]).max(v2[1]);
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let a0 = [v0[0] - cx, v0[1] - cy];
    let a1 = [v1[0] - cx, v1[1] - cy];
    let a2 = [v2[0] - cx, v2[1] - cy];
    out.push(CharQuad {
        glow: SLOT_TRI,
        uv: [a2[0], a2[1], 0.0, 0.0],
        px: [max_x - min_x, max_y - min_y],
        pos: [cx, cy],
        scale: 1.0,
        alpha,
        rotate: 0.0,
        color,
        ext: [a0[0], a0[1], a1[0], a1[1]],
    });
}

/// Apply a camera transform (zoom / pan / rotation) to all quads, around a centre point.
/// Used by sonnet for shot camera moves; leaves scale/rotation per quad composed.
pub fn apply_camera(quads: &mut [CharQuad], cx: f32, cy: f32, zoom: f32, tx: f32, ty: f32, rot: f32) {
    if zoom == 1.0 && tx == 0.0 && ty == 0.0 && rot == 0.0 {
        return;
    }
    let (cs, sn) = (rot.cos(), rot.sin());
    for q in quads {
        let dx = q.pos[0] - cx;
        let dy = q.pos[1] - cy;
        let rx = dx * cs - dy * sn;
        let ry = dx * sn + dy * cs;
        q.pos = [cx + rx * zoom + tx, cy + ry * zoom + ty];
        q.scale *= zoom;
        q.rotate += rot;
    }
}

/// Camera transform for quads in LOCAL coordinates (origin at the stage centre): maps
/// `pos` from stage-local space into screen space directly (no implicit re-centring).
pub fn apply_camera_local(quads: &mut [CharQuad], cx: f32, cy: f32, zoom: f32, tx: f32, ty: f32, rot: f32) {
    if zoom == 1.0 && tx == 0.0 && ty == 0.0 && rot == 0.0 {
        return;
    }
    let (cs, sn) = (rot.cos(), rot.sin());
    for q in quads {
        let rx = q.pos[0] * zoom;
        let ry = q.pos[1] * zoom;
        let rx2 = rx * cs - ry * sn;
        let ry2 = rx * sn + ry * cs;
        q.pos = [cx + rx2 + tx, cy + ry2 + ty];
        q.scale *= zoom;
        q.rotate += rot;
    }
}

/// Shared context handed to every style builder.
pub struct StyleCtx<'a> {
    pub width: f32,
    pub height: f32,
    pub time: f32,
    pub atlas: &'a GlyphAtlas,
    pub colors: &'a LyricColors,
    /// Deterministic animation seed (track id / song id).
    pub seed: u64,
    /// Sonnet MG decoration layer toggles (background / fixed-geometry / particles).
    pub mg_bg: bool,
    pub mg_fixed: bool,
    pub mg_decor: bool,
    /// Audio energy [bass, vocal, power] (0..1) for music-reactive particles.
    pub audio: [f32; 3],
    /// Post-processing tuning [grain, contrast, lens, rgbShift, halftone, vignette].
    pub post: [f32; 7],
    /// Manual global font weight (0 = per-role auto; 300/400/700/900 otherwise).
    pub font_weight: f32,
}

/// Lyrics + playback context a style needs.
pub struct StyleInput<'a> {
    pub lines: &'a [LyricLine],
    pub active_idx: usize,
    /// Active line's translation (empty when absent).
    pub translation: &'a str,
    /// Current track metadata for the end-of-song credits poster (may all be empty).
    pub song_title: &'a str,
    pub song_artist: &'a str,
    pub song_album: &'a str,
}

/// Font size conventions shared by styles.
pub struct FontScales {
    pub main: f32,
    pub context: f32,
    pub subtitle: f32,
}

impl FontScales {
    pub fn from_height(height: f32) -> Self {
        let min_d = height;
        Self {
            // Bumped from 0.12 → 0.16 to match folia's `5.4vw` main lyric size on a 1080p+
            // screen (folia uses clamp(2rem, 5.4vw, 5.6rem); 0.16 × 1455 ≈ 233px ≈ 5.4vw
            // at 2560 screen width). Bigger main = more readable, no more "tiny hero".
            main: min_d * 0.16,
            context: min_d * 0.078,
            subtitle: min_d * 0.04,
        }
    }
}

/// Dispatch to the style's frame builder. Add a new animation by adding a `LyricStyle` arm
/// here plus a `pub fn build_frame` in `crate::lyricstyles::<name>`.
pub fn build_frame(
    style: crate::config::LyricStyle,
    ctx: &StyleCtx,
    input: &StyleInput,
) -> StyleOutput {
    match style {
        crate::config::LyricStyle::Off => StyleOutput::empty(),
        crate::config::LyricStyle::Sonnet => crate::lyricstyles::sonnet::build_frame(ctx, input),
    }
}
