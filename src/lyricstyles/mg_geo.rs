//! Port of folia `sonnetShotMg.ts` (HUD + geometric chaos 0–17) and `sonnetAdditionalShotMg.ts`
//! (poster variants 18–23) plus the prism recipes from `sonnetSpatialMgGeometry.ts`.

use crate::lyricstyles::mg::{MgCanvas, shot_mg_bleed};

pub type Color = [f32; 4];

/// Pack rgb (0..1) + alpha into an RGBA colour.
pub fn rgba(rgb: [f32; 3], a: f32) -> Color {
    [rgb[0], rgb[1], rgb[2], a]
}

// ------------------------------------------------------------------ prisms

fn trace_polygon(t: &mut MgCanvas, points: &[[f32; 2]]) {
    t.move_to(points[0][0], points[0][1]);
    for p in &points[1..] {
        t.line_to(p[0], p[1]);
    }
    t.line_to(points[0][0], points[0][1]);
}

fn draw_extruded_polygon(t: &mut MgCanvas, front: &[[f32; 2]], depth_x: f32, depth_y: f32, color: Color, alpha: f32) {
    let back: Vec<[f32; 2]> = front.iter().map(|[x, y]| [x + depth_x, y + depth_y]).collect();
    for i in 0..front.len() {
        let next = (i + 1) % front.len();
        let face = [
            front[i], back[i], back[next], front[next],
        ];
        trace_polygon(t, &face);
        t.fill(color, alpha * (0.34 + (i % 3) as f32 * 0.12));
    }
    trace_polygon(t, front);
    t.fill(color, alpha * 0.22);
}

pub fn draw_solid_cuboid(t: &mut MgCanvas, x: f32, y: f32, width: f32, height: f32, depth_x: f32, depth_y: f32, color: Color, alpha: f32) {
    let left = x - width / 2.0;
    let right = x + width / 2.0;
    let top = y - height / 2.0;
    let bottom = y + height / 2.0;
    let front = [
        [left, top], [right, top], [right, bottom], [left, bottom],
    ];
    trace_polygon(t, &[[left, top], [left + depth_x, top + depth_y], [right + depth_x, top + depth_y], [right, top]]);
    t.fill(color, alpha * 0.42);
    trace_polygon(t, &[[right, top], [right + depth_x, top + depth_y], [right + depth_x, bottom + depth_y], [right, bottom]]);
    t.fill(color, alpha * 0.68);
    trace_polygon(t, &front);
    t.fill(color, alpha * 0.24);
}

pub fn draw_triangular_prism(t: &mut MgCanvas, x: f32, y: f32, width: f32, height: f32, depth_x: f32, depth_y: f32, color: Color, alpha: f32) {
    draw_extruded_polygon(t, &[
        [x, y - height / 2.0],
        [x + width / 2.0, y + height / 2.0],
        [x - width / 2.0, y + height / 2.0],
    ], depth_x, depth_y, color, alpha);
}

pub fn draw_hexagonal_prism(t: &mut MgCanvas, x: f32, y: f32, width: f32, height: f32, depth_x: f32, depth_y: f32, color: Color, alpha: f32) {
    draw_extruded_polygon(t, &[
        [x - width * 0.25, y - height / 2.0],
        [x + width * 0.25, y - height / 2.0],
        [x + width / 2.0, y],
        [x + width * 0.25, y + height / 2.0],
        [x - width * 0.25, y + height / 2.0],
        [x - width / 2.0, y],
    ], depth_x, depth_y, color, alpha);
}

pub fn draw_trapezoid_prism(t: &mut MgCanvas, x: f32, y: f32, top_width: f32, bottom_width: f32, height: f32, depth_x: f32, depth_y: f32, color: Color, alpha: f32) {
    draw_extruded_polygon(t, &[
        [x - top_width / 2.0, y - height / 2.0],
        [x + top_width / 2.0, y - height / 2.0],
        [x + bottom_width / 2.0, y + height / 2.0],
        [x - bottom_width / 2.0, y + height / 2.0],
    ], depth_x, depth_y, color, alpha);
}

// ------------------------------------------------------------------ HUD background layer

fn draw_cross(t: &mut MgCanvas, x: f32, y: f32, size: f32, color: Color, alpha: f32) {
    t.move_to(x - size, y - size).line_to(x + size, y + size).stroke(color, 1.0, alpha);
    t.move_to(x + size, y - size).line_to(x - size, y + size).stroke(color, 1.0, alpha);
}

fn draw_hatching(t: &mut MgCanvas, x: f32, y: f32, w: f32, h: f32, spacing: f32, color: Color) {
    let mut i = -w;
    while i < w + h {
        t.move_to(x + i, y).line_to(x + i + h, y + h).stroke(color, 1.0, 0.15);
        i += spacing;
    }
}

/// Always-on HUD decoration: corner crosses, left-edge repeats, bottom progress bar.
pub fn hud_bg(t: &mut MgCanvas, width: f32, height: f32, radius: f32, primary: Color, secondary: Color) {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let margin_x = width * 0.05;
    let margin_y = height * 0.05;
    draw_cross(t, -hw + margin_x, -hh + margin_y, 4.0, primary, 0.4);
    draw_cross(t, hw - margin_x, -hh + margin_y, 4.0, primary, 0.4);
    draw_cross(t, -hw + margin_x, hh - margin_y, 4.0, primary, 0.4);
    draw_cross(t, hw - margin_x, hh - margin_y, 4.0, primary, 0.4);
    for i in 0..8 {
        draw_cross(t, -hw + margin_x, -hh + margin_y + i as f32 * 20.0 + 30.0, 3.0, primary, 0.3);
    }
    let bar_y = hh - margin_y - 10.0;
    t.move_to(-hw + margin_x + 20.0, bar_y).line_to(hw - margin_x - 20.0, bar_y).stroke(primary, 1.0, 0.3);
    draw_cross(t, -hw + margin_x + 10.0, bar_y, 3.0, primary, 0.5);
    draw_cross(t, -hw + margin_x + 30.0, bar_y, 3.0, primary, 0.5);
    draw_cross(t, hw - margin_x - 10.0, bar_y, 3.0, primary, 0.5);
    t.circle(0.0, bar_y, 2.0).fill(secondary, 0.8);
    let _ = radius;
}

/// Editorial-column backdrop: strict grids + a frame.
pub fn editorial_bg(t: &mut MgCanvas, width: f32, height: f32, primary: Color) {
    let hw = width / 2.0;
    let hh = height / 2.0;
    for i in 1..=6 {
        let x = -hw + width * (i as f32 / 7.0);
        t.move_to(x, -hh).line_to(x, hh).stroke(primary, 1.0, 0.15);
    }
    for i in 1..=4 {
        let y = -hh + height * (i as f32 / 5.0);
        t.move_to(-hw, y).line_to(hw, y).stroke(primary, 1.0, 0.15);
    }
    t.rect(-hw + width * 0.2, -hh + height * 0.2, width * 0.6, height * 0.6).stroke(primary, 4.0, 0.5);
}

/// quiet-tableau / mask-reveal backdrop: scattered small rects.
pub fn scatter_bg(t: &mut MgCanvas, width: f32, height: f32, seed: i64, primary: Color) {
    let hw = width / 2.0;
    let hh = height / 2.0;
    for i in 0..5 {
        let size = 10.0 + (seed % (i + 1) as i64) as f32 * 5.0;
        let x = -hw + width * (0.2 + ((seed * 11 + i) % 60) as f32 / 100.0);
        let y = -hh + height * (0.2 + ((seed * 17 + i) % 60) as f32 / 100.0);
        t.rect(x, y, size, size).fill(primary, 0.4);
    }
}

// ------------------------------------------------------------------ geometric chaos variants

fn geo_rot(seed: i64, variant: i64) -> f32 {
    let keeps_upright = matches!(variant, 6 | 8 | 9 | 14 | 15 | 16 | 17 | 20 | 22 | 23) || variant >= 24;
    if !keeps_upright {
        ((seed * 13) % 360) as f32 * std::f32::consts::PI / 180.0
    } else if variant == 8 {
        // 90-degree increments
        let q = ((seed.div_euclid(48) % 4) + 4) % 4;
        q as f32 * std::f32::consts::FRAC_PI_2
    } else {
        0.0
    }
}

pub fn geo_variant_rotation(seed: i64, variant: i64) -> f32 {
    geo_rot(seed, variant)
}

pub fn draw_geo_variant(
    t: &mut MgCanvas,
    variant: i64,
    width: f32,
    height: f32,
    radius: f32,
    seed: i64,
    primary: Color,
    secondary: Color,
) -> bool {
    match variant {
        0 => {
            // Huge circular frame with sunburst
            t.circle(0.0, 0.0, radius * 0.6).stroke(primary, 6.0, 0.8);
            t.circle(0.0, 0.0, radius * 0.58).stroke(primary, 2.0, 0.4);
            for i in 0..32 {
                let angle = (i as f32 / 32.0) * std::f32::consts::TAU;
                let r1 = radius * (0.3 + (i % 3) as f32 * 0.05);
                let r2 = radius * 0.55;
                t.move_to(angle.cos() * r1, angle.sin() * r1)
                    .line_to(angle.cos() * r2, angle.sin() * r2)
                    .stroke(primary, 1.0, 0.2 + (i % 2) as f32 * 0.1);
            }
        }
        1 => {
            // Nested diamonds
            let r = radius * 0.7;
            t.move_to(0.0, -r).line_to(r, 0.0).line_to(0.0, r).line_to(-r, 0.0).line_to(0.0, -r).stroke(primary, 6.0, 0.8);
            t.move_to(0.0, -r * 0.96).line_to(r * 0.96, 0.0).line_to(0.0, r * 0.96).line_to(-r * 0.96, 0.0).line_to(0.0, -r * 0.96).stroke(primary, 2.0, 0.4);
            t.move_to(0.0, -r * 0.4).line_to(r * 0.4, 0.0).line_to(0.0, r * 0.4).line_to(-r * 0.4, 0.0).line_to(0.0, -r * 0.4).stroke(primary, 1.0, 0.6);
            t.move_to(-r, 0.0).line_to(r, 0.0).stroke(primary, 1.0, 0.3);
            t.move_to(0.0, -r).line_to(0.0, r).stroke(primary, 1.0, 0.3);
        }
        2 => {
            // Tech hexagon grid
            let mut draw_hex = |t: &mut MgCanvas, x: f32, y: f32, r: f32, w: f32, a: f32| {
                t.move_to(x + r * (0.0f32).sin(), y - r * (0.0f32).cos());
                for j in 1..=6 {
                    t.line_to(x + r * (j as f32 * std::f32::consts::FRAC_PI_3).sin(), y - r * (j as f32 * std::f32::consts::FRAC_PI_3).cos());
                }
                t.stroke(primary, w, a);
            };
            draw_hex(t, 0.0, 0.0, radius * 0.6, 6.0, 0.8);
            draw_hex(t, 0.0, 0.0, radius * 0.57, 2.0, 0.4);
            draw_hex(t, 0.0, 0.0, radius * 0.25, 1.0, 0.5);
            for j in 0..6 {
                let angle = j as f32 * std::f32::consts::FRAC_PI_3 - std::f32::consts::FRAC_PI_6;
                t.move_to(angle.cos() * radius * 0.25, angle.sin() * radius * 0.25)
                    .line_to(angle.cos() * radius * 0.57, angle.sin() * radius * 0.57)
                    .stroke(primary, 2.0, 0.4);
            }
        }
        3 => {
            // Organic molecules
            let mol_variant = (seed.div_euclid(48) % 3).rem_euclid(3);
            if mol_variant == 0 {
                // Benzene cluster
                let hex_r = radius * 0.22;
                let r_main = hex_r * 1.2;
                let mut draw_benzene = |t: &mut MgCanvas, cx: f32, cy: f32, scale: f32| {
                    let r = hex_r * scale;
                    t.move_to(cx + r * (0.0f32).sin(), cy - r * (0.0f32).cos());
                    for j in 1..=6 {
                        t.line_to(cx + r * (j as f32 * std::f32::consts::FRAC_PI_3).sin(), cy - r * (j as f32 * std::f32::consts::FRAC_PI_3).cos());
                    }
                    t.stroke(primary, 3.0, 0.8);
                    for j in (0..6).step_by(2) {
                        let inner_r = r * 0.82;
                        t.move_to(cx + inner_r * (j as f32 * std::f32::consts::FRAC_PI_3).sin(), cy - inner_r * (j as f32 * std::f32::consts::FRAC_PI_3).cos())
                            .line_to(cx + inner_r * ((j + 1) as f32 * std::f32::consts::FRAC_PI_3).sin(), cy - inner_r * ((j + 1) as f32 * std::f32::consts::FRAC_PI_3).cos())
                            .stroke(primary, 2.0, 0.5);
                    }
                };
                draw_benzene(t, 0.0, 0.0, 1.2);
                let dx = (std::f32::consts::FRAC_PI_3).sin() * r_main * 2.0;
                draw_benzene(t, dx, 0.0, 1.2);
                let branch_dist = (std::f32::consts::FRAC_PI_3).sin() * r_main * 2.0;
                draw_benzene(t, -(std::f32::consts::FRAC_PI_6).sin() * branch_dist, -(std::f32::consts::FRAC_PI_6).cos() * branch_dist, 1.2);
                t.move_to(0.0, r_main)
                    .line_to(0.0, r_main + radius * 0.2)
                    .line_to(radius * 0.15, r_main + radius * 0.35)
                    .stroke(primary, 2.0, 0.6);
            } else if mol_variant == 1 {
                // Caffeine-like fused hexagon + pentagon + branches
                let hex_r = radius * 0.22;
                t.move_to(0.0, -hex_r);
                for j in 1..=6 {
                    t.line_to(hex_r * (j as f32 * std::f32::consts::FRAC_PI_3).sin(), -hex_r * (j as f32 * std::f32::consts::FRAC_PI_3).cos());
                }
                t.stroke(primary, 3.0, 0.8);
                let a1 = (std::f32::consts::FRAC_PI_3).sin();
                let a2 = (2.0f32 * std::f32::consts::FRAC_PI_3).sin();
                let a4 = (4.0f32 * std::f32::consts::FRAC_PI_3).sin();
                let a5 = (5.0f32 * std::f32::consts::FRAC_PI_3).cos();
                t.move_to(hex_r * 0.8 * a1, -hex_r * 0.8 * (std::f32::consts::FRAC_PI_3).cos())
                    .line_to(hex_r * 0.8 * a2, -hex_r * 0.8 * (2.0f32 * std::f32::consts::FRAC_PI_3).cos())
                    .stroke(primary, 2.0, 0.5);
                t.move_to(hex_r * 0.8 * a4, -hex_r * 0.8 * (4.0f32 * std::f32::consts::FRAC_PI_3).cos())
                    .line_to(hex_r * 0.8 * (5.0f32 * std::f32::consts::FRAC_PI_3).sin(), -hex_r * 0.8 * a5)
                    .stroke(primary, 2.0, 0.5);
                let px1 = hex_r * (3.0f32).sqrt() / 2.0;
                let py1 = -hex_r / 2.0;
                let px2 = hex_r * (3.0f32).sqrt() / 2.0;
                let py2 = hex_r / 2.0;
                let pent_top_x = px1 + hex_r * 0.8;
                let pent_top_y = py1 - hex_r * 0.1;
                let pent_mid_x = px1 + hex_r * 1.2;
                let pent_mid_y = 0.0;
                let pent_bot_x = px2 + hex_r * 0.8;
                let pent_bot_y = py2 + hex_r * 0.1;
                t.move_to(px1, py1).line_to(pent_top_x, pent_top_y).line_to(pent_mid_x, pent_mid_y)
                    .line_to(pent_bot_x, pent_bot_y).line_to(px2, py2).stroke(primary, 3.0, 0.8);
                let mut draw_branch = |t: &mut MgCanvas, sx: f32, sy: f32, angle: f32, len: f32, node: bool| {
                    let ex = sx + angle.cos() * len;
                    let ey = sy + angle.sin() * len;
                    t.move_to(sx, sy).line_to(ex, ey).stroke(primary, 2.0, 0.6);
                    if node {
                        t.circle(ex, ey, 6.0).stroke(primary, 2.0, 0.8);
                    }
                };
                draw_branch(t, 0.0, -hex_r, -std::f32::consts::FRAC_PI_2, radius * 0.15, true);
                draw_branch(t, -hex_r * (3.0f32).sqrt() / 2.0, hex_r / 2.0, std::f32::consts::PI * 0.8, radius * 0.2, true);
                draw_branch(t, -hex_r * (3.0f32).sqrt() / 2.0, -hex_r / 2.0, -std::f32::consts::PI * 0.8, radius * 0.15, false);
                draw_branch(t, pent_mid_x, pent_mid_y, 0.0, radius * 0.18, false);
                draw_branch(t, pent_mid_x + radius * 0.18, pent_mid_y, std::f32::consts::FRAC_PI_4, radius * 0.1, true);
            } else {
                // Linear polymer chain
                let seg_len = radius * 0.18;
                let steps = 7;
                let start_x = -seg_len * (steps as f32 / 2.0) * (std::f32::consts::FRAC_PI_6).cos();
                let mut pts: Vec<[f32; 2]> = Vec::new();
                let mut cx = start_x;
                let mut cy = 0.0;
                pts.push([cx, cy]);
                for i in 0..steps {
                    cx += seg_len * (std::f32::consts::FRAC_PI_6).cos();
                    cy = (if i % 2 == 0 { 1.0 } else { -1.0 }) * seg_len * (std::f32::consts::FRAC_PI_6).sin();
                    pts.push([cx, cy]);
                }
                t.move_to(pts[0][0], pts[0][1]);
                for i in 1..=steps {
                    t.line_to(pts[i][0], pts[i][1]);
                }
                t.stroke(primary, 3.0, 0.8);
                let nx = -(std::f32::consts::FRAC_PI_6).sin() * 6.0;
                let ny = (std::f32::consts::FRAC_PI_6).cos() * 6.0;
                t.move_to(pts[1][0] + nx, pts[1][1] + ny).line_to(pts[2][0] + nx, pts[2][1] + ny).stroke(primary, 2.0, 0.5);
                for i in 1..steps {
                    let angle = if i % 2 == 0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
                    let bx = pts[i][0];
                    let by = pts[i][1] + angle.sin() * seg_len * 0.6;
                    t.move_to(pts[i][0], pts[i][1]).line_to(bx, by).stroke(primary, 2.0, 0.5);
                    if i % 2 != 0 {
                        t.circle(bx, by, 5.0).stroke(primary, 2.0, 0.8);
                    } else {
                        t.move_to(bx, by).line_to(bx + seg_len * 0.5, by - seg_len * 0.3).stroke(primary, 2.0, 0.5);
                    }
                }
            }
        }
        4 => {
            // Atomic electron orbitals
            let ell_r = radius * 0.7;
            for i in 0..3 {
                let angle = i as f32 * std::f32::consts::FRAC_PI_3;
                let steps = 60;
                for j in 0..=steps {
                    let tt = j as f32 * std::f32::consts::TAU / steps as f32;
                    let ex = tt.cos() * ell_r;
                    let ey = tt.sin() * ell_r * 0.18;
                    let rx = ex * angle.cos() - ey * angle.sin();
                    let ry = ex * angle.sin() + ey * angle.cos();
                    if j == 0 {
                        t.move_to(rx, ry);
                    } else {
                        t.line_to(rx, ry);
                    }
                }
                t.stroke(primary, 1.0, 0.3);
            }
            t.circle(0.0, 0.0, radius * 0.05).fill(primary, 0.8);
        }
        5 => {
            // Planet with rings & orbits
            let planet_r = radius * 0.25;
            t.circle(0.0, 0.0, planet_r).fill(primary, 0.15).stroke(primary, 2.0, 0.8);
            t.move_to(-planet_r * 0.7, -planet_r * 0.5).quad_to(0.0, -planet_r * 0.2, planet_r * 0.7, -planet_r * 0.5).stroke(primary, 1.0, 0.4);
            t.move_to(-planet_r * 0.9, 0.0).quad_to(0.0, planet_r * 0.3, planet_r * 0.9, 0.0).stroke(primary, 1.0, 0.4);
            let ring_rx = radius * 0.6;
            let ring_ry = radius * 0.15;
            let angle = std::f32::consts::FRAC_PI_6;
            let mut draw_tilted_ellipse = |t: &mut MgCanvas, rx: f32, ry: f32, w: f32, a: f32| {
                let segments = 60;
                for j in 0..=segments {
                    let tt = j as f32 * std::f32::consts::TAU / segments as f32;
                    let ex = tt.cos() * rx;
                    let ey = tt.sin() * ry;
                    let rot_x = ex * angle.cos() - ey * angle.sin();
                    let rot_y = ex * angle.sin() + ey * angle.cos();
                    if j == 0 {
                        t.move_to(rot_x, rot_y);
                    } else {
                        t.line_to(rot_x, rot_y);
                    }
                }
                t.stroke(primary, w, a);
            };
            draw_tilted_ellipse(t, ring_rx, ring_ry, 4.0, 0.5);
            draw_tilted_ellipse(t, ring_rx * 1.1, ring_ry * 1.15, 1.0, 0.3);
            draw_tilted_ellipse(t, ring_rx * 1.25, ring_ry * 1.3, 2.0, 0.2);
            t.circle(0.0, 0.0, radius * 0.7).stroke(primary, 1.0, 0.2);
            let dot_angle = std::f32::consts::FRAC_PI_4;
            t.circle(dot_angle.cos() * radius * 0.7, dot_angle.sin() * radius * 0.7, 8.0).fill(primary, 0.6);
        }
        6 => {
            // Abstract wireframe mountains
            let bleed = shot_mg_bleed(width, height, radius);
            let w = bleed[0] * 1.08;
            let h = radius * 0.8;
            let base_y = radius * 0.2;
            t.circle(0.0, base_y - h * 0.6, radius * 0.3).stroke(primary, 2.0, 0.4);
            for i in 0..5 {
                t.move_to(-bleed[0], base_y - h * 0.6 + i as f32 * 15.0).line_to(bleed[0], base_y - h * 0.6 + i as f32 * 15.0).stroke(primary, 1.0, 0.3);
            }
            let peaks = 7;
            for layer in 0..3 {
                let layer_w = w * (1.0 + layer as f32 * 0.2);
                let layer_h = h * (0.5 + layer as f32 * 0.25);
                t.move_to(-layer_w / 2.0, base_y);
                for i in 1..peaks {
                    let px = -layer_w / 2.0 + (layer_w / peaks as f32) * i as f32;
                    let py = base_y - layer_h * (0.3 + 0.7 * ((seed + layer * 11 + i * 7) as f32).sin().abs());
                    t.line_to(px, py);
                }
                t.line_to(layer_w / 2.0, base_y);
                t.stroke(primary, 3.0 - layer as f32, 0.6 - layer as f32 * 0.15);
            }
            t.move_to(-w, base_y).line_to(w, base_y).stroke(primary, 4.0, 0.8);
            for i in 0..5 {
                let grid_y = base_y + (i as f32).powf(1.5) * 12.0;
                t.move_to(-w, grid_y).line_to(w, grid_y).stroke(primary, 1.0, 0.4 - i as f32 * 0.08);
            }
            for i in -4..=4 {
                t.move_to(i as f32 * radius * 0.2, base_y).line_to(i as f32 * bleed[0] * 0.32, bleed[1]).stroke(primary, 1.0, 0.3);
            }
        }
        7 => {
            // Radar / concentric target
            for i in 1..=6 {
                let r = radius * 0.15 * i as f32;
                t.circle(0.0, 0.0, r).stroke(primary, if i % 2 == 0 { 2.0 } else { 1.0 }, 0.2 + (i % 3) as f32 * 0.1);
            }
            t.move_to(-radius * 0.9, 0.0).line_to(radius * 0.9, 0.0).stroke(primary, 1.0, 0.4);
            t.move_to(0.0, -radius * 0.9).line_to(0.0, radius * 0.9).stroke(primary, 1.0, 0.4);
            t.move_to(0.0, 0.0);
            t.arc(0.0, 0.0, radius * 0.75, 0.0, std::f32::consts::FRAC_PI_4, false);
            t.line_to(0.0, 0.0);
            t.fill(primary, 0.1);
            t.move_to(0.0, 0.0);
            t.arc(0.0, 0.0, radius * 0.75, 0.0, std::f32::consts::FRAC_PI_4, false);
            t.stroke(primary, 2.0, 0.5);
            let r_outer = radius * 0.8;
            for i in 0..72 {
                let angle = (i as f32 / 72.0) * std::f32::consts::TAU;
                let len = if i % 18 == 0 { 20.0 } else if i % 6 == 0 { 10.0 } else { 5.0 };
                t.move_to(angle.cos() * r_outer, angle.sin() * r_outer)
                    .line_to(angle.cos() * (r_outer + len), angle.sin() * (r_outer + len))
                    .stroke(primary, 1.0, 0.4);
            }
            let lock_angle = (seed % 360) as f32 * std::f32::consts::PI / 180.0;
            let lock_r = radius * 0.45;
            let lx = lock_angle.cos() * lock_r;
            let ly = lock_angle.sin() * lock_r;
            t.rect(lx - 15.0, ly - 15.0, 30.0, 30.0).stroke(primary, 2.0, 0.8);
            t.move_to(lx, ly - 20.0).line_to(lx, ly + 20.0).stroke(primary, 1.0, 0.6);
            t.move_to(lx - 20.0, ly).line_to(lx + 20.0, ly).stroke(primary, 1.0, 0.6);
        }
        8 => {
            // Technical HUD decorative frame
            let fw = radius * 0.85;
            let fh = radius * 0.65;
            let bracket_size = radius * 0.15;
            let draw_bracket = |t: &mut MgCanvas, cx: f32, cy: f32, sx: f32, sy: f32| {
                t.move_to(cx - sx * bracket_size, cy)
                    .line_to(cx, cy)
                    .line_to(cx, cy - sy * bracket_size)
                    .stroke(primary, 3.0, 0.7);
                t.move_to(cx - sx * bracket_size * 0.8, cy - sy * 8.0)
                    .line_to(cx - sx * 8.0, cy - sy * 8.0)
                    .line_to(cx - sx * 8.0, cy - sy * bracket_size * 0.8)
                    .stroke(primary, 1.0, 0.4);
            };
            draw_bracket(t, -fw, -fh, -1.0, -1.0);
            draw_bracket(t, fw, -fh, 1.0, -1.0);
            draw_bracket(t, -fw, fh, -1.0, 1.0);
            draw_bracket(t, fw, fh, 1.0, 1.0);
            let mut i = -fw + 20.0;
            while i < fw - 20.0 {
                t.move_to(i, -fh).line_to(i, -fh - (if (i as i64 % 60) == 0 { 12.0 } else { 6.0 })).stroke(primary, 1.0, 0.5);
                t.move_to(i, fh).line_to(i, fh + (if (i as i64 % 60) == 0 { 12.0 } else { 6.0 })).stroke(primary, 1.0, 0.5);
                i += 20.0;
            }
            t.circle(0.0, 0.0, radius * 0.1).stroke(primary, 2.0, 0.4);
            t.move_to(-radius * 0.15, 0.0).line_to(radius * 0.15, 0.0).stroke(primary, 1.0, 0.4);
            t.move_to(0.0, -radius * 0.15).line_to(0.0, radius * 0.15).stroke(primary, 1.0, 0.4);
            t.rect(-fw, -fh + 20.0, 10.0, 40.0).fill(primary, 0.5);
            t.rect(-fw, -fh + 65.0, 10.0, 15.0).fill(primary, 0.3);
            t.rect(fw - 10.0, fh - 60.0, 10.0, 40.0).fill(primary, 0.5);
        }
        9 => {
            // Isometric cubes
            let draw_cube = |t: &mut MgCanvas, cx: f32, cy: f32, size: f32, alpha: f32| {
                let dy = size * 0.5;
                let dx = size * 0.866;
                t.move_to(cx, cy - size).line_to(cx + dx, cy - dy).line_to(cx, cy).line_to(cx - dx, cy - dy).line_to(cx, cy - size)
                    .fill(primary, alpha * 0.15).stroke(primary, 2.0, alpha * 0.8);
                t.move_to(cx, cy).line_to(cx + dx, cy - dy).line_to(cx + dx, cy + size - dy).line_to(cx, cy + size).line_to(cx, cy)
                    .fill(primary, alpha * 0.3).stroke(primary, 2.0, alpha * 0.8);
                t.move_to(cx, cy).line_to(cx - dx, cy - dy).line_to(cx - dx, cy + size - dy).line_to(cx, cy + size).line_to(cx, cy)
                    .fill(primary, alpha * 0.05).stroke(primary, 2.0, alpha * 0.8);
            };
            draw_cube(t, 0.0, 0.0, radius * 0.35, 0.8);
            draw_cube(t, radius * 0.4, -radius * 0.15, radius * 0.2, 0.5);
            draw_cube(t, -radius * 0.45, radius * 0.25, radius * 0.25, 0.6);
            draw_cube(t, 0.0, radius * 0.45, radius * 0.15, 0.4);
            t.move_to(0.0, 0.0).line_to(radius * 0.4, -radius * 0.15).stroke(primary, 1.0, 0.3);
            t.move_to(0.0, 0.0).line_to(-radius * 0.45, radius * 0.25).stroke(primary, 1.0, 0.3);
        }
        10 => {
            // Constellation network
            t.circle(0.0, 0.0, radius * 0.75).stroke(primary, 1.0, 0.2);
            t.circle(0.0, 0.0, radius * 0.73).stroke(primary, 2.0, 0.1);
            let mut nodes: Vec<[f32; 2]> = Vec::new();
            for i in 0..18 {
                let r = radius * (0.1 + ((seed * 17 + i * 23) % 65) as f32 / 100.0);
                let angle = ((seed * 11 + i * 37) % 360) as f32 * std::f32::consts::PI / 180.0;
                nodes.push([angle.cos() * r, angle.sin() * r]);
            }
            for i in 0..nodes.len() {
                t.circle(nodes[i][0], nodes[i][1], 3.0).fill(primary, 0.7);
                t.circle(nodes[i][0], nodes[i][1], 6.0).stroke(primary, 1.0, 0.3);
                for j in i + 1..nodes.len() {
                    let dist = (nodes[i][0] - nodes[j][0]).hypot(nodes[i][1] - nodes[j][1]);
                    if dist < radius * 0.45 {
                        t.move_to(nodes[i][0], nodes[i][1])
                            .line_to(nodes[j][0], nodes[j][1])
                            .stroke(primary, 1.0, 0.4 * (1.0 - dist / (radius * 0.45)));
                    }
                }
            }
        }
        11 => {
            // Moon and lunar phases
            let moon_r = radius * 0.4;
            t.move_to(0.0, -moon_r);
            t.arc(0.0, 0.0, moon_r, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, false);
            t.quad_to(-moon_r * 0.4, 0.0, 0.0, -moon_r);
            t.fill(primary, 0.8);
            t.circle(0.0, 0.0, moon_r).stroke(primary, 1.0, 0.3);
            let orbit_r = radius * 0.65;
            t.circle(0.0, 0.0, orbit_r).stroke(primary, 1.0, 0.2);
            for i in 0..8 {
                let angle = (i as f32 / 8.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let mx = angle.cos() * orbit_r;
                let my = angle.sin() * orbit_r;
                t.circle(mx, my, 8.0).stroke(primary, 1.0, 0.5);
                if i == 0 {
                    // new moon (empty)
                } else if i == 4 {
                    t.circle(mx, my, 6.0).fill(primary, 0.8);
                } else {
                    t.move_to(mx, my - 6.0);
                    t.arc(mx, my, 6.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, i > 4);
                    t.line_to(mx, my - 6.0);
                    t.fill(primary, 0.5);
                }
            }
            let mut draw_star = |t: &mut MgCanvas, sx: f32, sy: f32, sr: f32| {
                t.move_to(sx, sy - sr).line_to(sx + sr * 0.2, sy - sr * 0.2)
                    .line_to(sx + sr, sy).line_to(sx + sr * 0.2, sy + sr * 0.2)
                    .line_to(sx, sy + sr).line_to(sx - sr * 0.2, sy + sr * 0.2)
                    .line_to(sx - sr, sy).line_to(sx - sr * 0.2, sy - sr * 0.2)
                    .fill(primary, 0.7);
            };
            draw_star(t, -radius * 0.4, -radius * 0.5, 12.0);
            draw_star(t, radius * 0.5, -radius * 0.3, 8.0);
            draw_star(t, -radius * 0.2, radius * 0.5, 15.0);
        }
        12 => {
            // Geometric lotus
            let petal_len = radius * 0.5;
            let mut draw_petal = |t: &mut MgCanvas, angle: f32, length: f32, width: f32, alpha: f32| {
                let cx = 0.0f32;
                let cy = 0.0f32;
                let end_x = cx + angle.cos() * length;
                let end_y = cy + angle.sin() * length;
                let ctrl_dist = length * 0.5;
                let lca = angle - width;
                let c1x = cx + lca.cos() * ctrl_dist;
                let c1y = cy + lca.sin() * ctrl_dist;
                let rca = angle + width;
                let c2x = cx + rca.cos() * ctrl_dist;
                let c2y = cy + rca.sin() * ctrl_dist;
                t.move_to(cx, cy);
                t.quad_to(c1x, c1y, end_x, end_y);
                t.quad_to(c2x, c2y, cx, cy);
                t.stroke(primary, 1.0, alpha);
                if alpha > 0.5 {
                    t.move_to(cx, cy);
                    t.quad_to(c1x, c1y, end_x, end_y);
                    t.quad_to(c2x, c2y, cx, cy);
                    t.fill(primary, alpha * 0.2);
                }
            };
            for layer in 0..4 {
                let petals = 8 + layer * 4;
                let current_len = petal_len * (1.0 - layer as f32 * 0.2);
                let current_width = 0.3 - layer as f32 * 0.05;
                let offset = layer as f32 * (std::f32::consts::PI / petals as f32);
                for i in 0..petals {
                    let angle = (i as f32 / petals as f32) * std::f32::consts::TAU + offset;
                    draw_petal(t, angle, current_len, current_width, 0.8 - layer as f32 * 0.15);
                }
            }
            t.circle(0.0, 0.0, radius * 0.08).stroke(primary, 2.0, 0.8);
            t.circle(0.0, 0.0, radius * 0.03).fill(primary, 0.9);
            let stem_r = radius * 0.65;
            for i in 0..3 {
                let start_angle = (i as f32 / 3.0) * std::f32::consts::TAU;
                t.move_to(start_angle.cos() * stem_r, start_angle.sin() * stem_r);
                t.cubic_to(
                    (start_angle + 1.0).cos() * stem_r * 1.2, (start_angle + 1.0).sin() * stem_r * 1.2,
                    (start_angle + 2.0).cos() * stem_r * 0.8, (start_angle + 2.0).sin() * stem_r * 0.8,
                    (start_angle + 3.0).cos() * stem_r, (start_angle + 3.0).sin() * stem_r,
                ).stroke(primary, 1.0, 0.4);
                let lx = (start_angle + 1.5).cos() * stem_r * 1.05;
                let ly = (start_angle + 1.5).sin() * stem_r * 1.05;
                t.circle(lx, ly, 4.0).fill(primary, 0.6);
            }
        }
        13 => {
            // Sacred geometry (seed of life)
            let sofl_r = radius * 0.25;
            t.circle(0.0, 0.0, sofl_r).stroke(primary, 1.5, 0.35);
            for i in 0..6 {
                let angle = (i as f32 / 6.0) * std::f32::consts::TAU;
                let cx = angle.cos() * sofl_r;
                let cy = angle.sin() * sofl_r;
                t.circle(cx, cy, sofl_r).stroke(primary, 1.5, 0.2);
            }
            for i in 0..12 {
                let angle = (i as f32 / 12.0) * std::f32::consts::TAU;
                let dist = if i % 2 == 0 { sofl_r * 2.0 } else { sofl_r * (3.0f32).sqrt() };
                let cx = angle.cos() * dist;
                let cy = angle.sin() * dist;
                t.circle(cx, cy, sofl_r).stroke(secondary, 1.0, 0.1);
            }
            t.circle(0.0, 0.0, sofl_r * 3.0).stroke(primary, 1.5, 0.2);
            t.circle(0.0, 0.0, sofl_r * 3.1).stroke(secondary, 1.0, 0.08);
            for i in 0..12 {
                let angle = (i as f32 / 12.0) * std::f32::consts::TAU;
                t.move_to(angle.cos() * sofl_r * 0.5, angle.sin() * sofl_r * 0.5)
                    .line_to(angle.cos() * sofl_r * 3.0, angle.sin() * sofl_r * 3.0)
                    .stroke(secondary, 1.0, 0.05);
            }
        }
        14 => {
            // Translucent architectural monoliths
            let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
            draw_solid_cuboid(t, radius * 0.18 * direction, radius * 0.03, radius * 0.62, radius * 0.7, radius * 0.22 * direction, -radius * 0.16, primary, 0.34);
            draw_solid_cuboid(t, -radius * 0.48 * direction, radius * 0.24, radius * 0.28, radius * 0.38, radius * 0.12 * direction, -radius * 0.09, primary, 0.24);
            draw_solid_cuboid(t, radius * 0.55 * direction, -radius * 0.3, radius * 0.2, radius * 0.26, radius * 0.09 * direction, -radius * 0.07, primary, 0.2);
        }
        15 => {
            // Floating triangular prisms
            let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
            draw_triangular_prism(t, -radius * 0.12 * direction, radius * 0.02, radius * 0.72, radius * 0.68, radius * 0.18 * direction, -radius * 0.13, primary, 0.34);
            draw_triangular_prism(t, radius * 0.48 * direction, radius * 0.26, radius * 0.28, radius * 0.25, -radius * 0.08 * direction, -radius * 0.06, primary, 0.22);
            draw_triangular_prism(t, -radius * 0.5 * direction, -radius * 0.3, radius * 0.2, radius * 0.18, radius * 0.06 * direction, -radius * 0.05, primary, 0.18);
        }
        16 => {
            // Faceted hexagonal solids
            let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
            draw_hexagonal_prism(t, radius * 0.12 * direction, 0.0, radius * 0.68, radius * 0.72, radius * 0.2 * direction, -radius * 0.14, primary, 0.32);
            draw_hexagonal_prism(t, -radius * 0.48 * direction, radius * 0.27, radius * 0.25, radius * 0.28, radius * 0.07 * direction, -radius * 0.05, primary, 0.2);
            draw_hexagonal_prism(t, radius * 0.52 * direction, -radius * 0.3, radius * 0.18, radius * 0.2, -radius * 0.06 * direction, -radius * 0.045, primary, 0.17);
        }
        17 => {
            // Trapezoid prisms and plinths
            let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
            draw_trapezoid_prism(t, radius * 0.12 * direction, radius * 0.04, radius * 0.3, radius * 0.68, radius * 0.62, radius * 0.18 * direction, -radius * 0.13, primary, 0.34);
            draw_trapezoid_prism(t, -radius * 0.42 * direction, radius * 0.28, radius * 0.2, radius * 0.38, radius * 0.22, radius * 0.08 * direction, -radius * 0.06, primary, 0.21);
            draw_trapezoid_prism(t, radius * 0.5 * direction, -radius * 0.3, radius * 0.2, radius * 0.12, radius * 0.22, -radius * 0.06 * direction, -radius * 0.05, primary, 0.18);
        }
        _ => {
            // 18..23 additional poster motifs; 24..47 themed / open-frame backgrounds.
            if draw_additional_variant(t, variant, width, height, radius, seed, primary, secondary) {
                return true;
            }
            return crate::lyricstyles::mg_themed::draw_variant_dispatch(t, variant, width, height, radius, seed, primary, secondary);
        }
    }
    true
}

// ------------------------------------------------------------------ additional variants 18..23

/// Contour atlas (18).
fn draw_contour_atlas(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let steps = 48;
    for ring in 0..8 {
        let base_radius = radius * (0.16 + ring as f32 * 0.075);
        for step in 0..=steps {
            let angle = (step as f32 / steps as f32) * std::f32::consts::TAU;
            let ripple = (angle * 3.0 + seed as f32 * 0.07 + ring as f32).sin() * radius * 0.018
                + (angle * 5.0 - seed as f32 * 0.03 + ring as f32 * 0.7).cos() * radius * 0.012;
            let x = angle.cos() * (base_radius + ripple) + (ring as f32 * 1.7).sin() * radius * 0.055;
            let y = angle.sin() * (base_radius + ripple) * 0.72 + (ring as f32 * 1.3).cos() * radius * 0.035;
            if step == 0 {
                t.move_to(x, y);
            } else {
                t.line_to(x, y);
            }
        }
        let color = if ring % 3 == 0 { secondary } else { primary };
        t.stroke(color, if ring % 3 == 0 { 2.0 } else { 1.0 }, 0.2 + ring as f32 * 0.045);
    }
    t.move_to(-radius * 0.78, radius * 0.52)
        .line_to(-radius * 0.58, radius * 0.52)
        .line_to(-radius * 0.58, radius * 0.46)
        .line_to(-radius * 0.38, radius * 0.46)
        .stroke(primary, 3.0, 0.64);
}

/// Radial wave (19).
fn draw_radial_wave(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bars = 64;
    for index in 0..bars {
        let angle = (index as f32 / bars as f32) * std::f32::consts::TAU;
        let signal = 0.5 + 0.5 * (index as f32 * 1.83 + seed as f32 * 0.11).sin();
        let inner = radius * (0.29 + signal * 0.035);
        let outer = radius * (0.43 + signal * 0.21);
        t.move_to(angle.cos() * inner, angle.sin() * inner)
            .line_to(angle.cos() * outer, angle.sin() * outer)
            .stroke(if index % 8 == 0 { secondary } else { primary }, if index % 8 == 0 { 3.0 } else { 1.0 }, if index % 2 == 0 { 0.58 } else { 0.32 });
    }
    t.circle(0.0, 0.0, radius * 0.24).stroke(primary, 5.0, 0.7);
    t.circle(0.0, 0.0, radius * 0.68).stroke(secondary, 1.0, 0.18);
    t.circle(0.0, 0.0, radius * 0.72).stroke(primary, 2.0, 0.12);
}

/// Transit blueprint (20).
fn draw_transit_blueprint(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    let routes: [&[[f32; 2]]; 3] = [
        &[[-0.72, -0.38], [-0.38, -0.38], [-0.38, 0.08], [0.08, 0.08], [0.08, 0.52], [0.68, 0.52]],
        &[[-0.62, 0.58], [-0.62, 0.24], [-0.14, 0.24], [-0.14, -0.52], [0.5, -0.52], [0.5, -0.2], [0.74, -0.2]],
        &[[-0.78, -0.06], [-0.5, -0.06], [-0.5, -0.62], [0.24, -0.62], [0.24, 0.3], [0.72, 0.3]],
    ];
    for (route_index, route) in routes.iter().enumerate() {
        let first = route[0];
        let last = route[route.len() - 1];
        t.move_to(-bleed[0] * direction, first[1] * radius);
        for [x, y] in *route {
            t.line_to(x * radius * direction, y * radius);
        }
        t.line_to(bleed[0] * direction, last[1] * radius);
        t.stroke(if route_index == 1 { secondary } else { primary }, if route_index == 0 { 5.0 } else { 2.0 }, 0.34 + route_index as f32 * 0.12);
        for (point_index, [x, y]) in route.iter().enumerate() {
            if point_index == 0 || point_index == route.len() - 1 || (point_index as i64 + route_index as i64) % 2 == 0 {
                let px = x * radius * direction;
                let py = y * radius;
                t.circle(px, py, if point_index == 0 { 10.0 } else { 6.0 }).fill(if point_index % 2 == 0 { primary } else { secondary }, 0.72);
                t.circle(px, py, if point_index == 0 { 16.0 } else { 11.0 }).stroke(primary, 1.0, 0.36);
            }
        }
    }
}

/// Chronograph (21).
fn draw_chronograph(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let rings = [0.2f32, 0.38, 0.62];
    for (index, &scale) in rings.iter().enumerate() {
        t.circle(0.0, 0.0, radius * scale).stroke(
            if index == 1 { secondary } else { primary },
            if index == 2 { 4.0 } else { 2.0 },
            0.3 + index as f32 * 0.13,
        );
    }
    for tick in 0..48 {
        let angle = (tick as f32 / 48.0) * std::f32::consts::TAU;
        let outer = radius * 0.72;
        let inner = outer - radius * if tick % 4 == 0 { 0.11 } else { 0.045 };
        t.move_to(angle.cos() * inner, angle.sin() * inner)
            .line_to(angle.cos() * outer, angle.sin() * outer)
            .stroke(if tick % 4 == 0 { secondary } else { primary }, if tick % 4 == 0 { 3.0 } else { 1.0 }, 0.5);
    }
    let hand_angle = ((seed % 60) as f32 / 60.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let second_angle = (((seed * 7) % 60) as f32 / 60.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    t.move_to(-hand_angle.cos() * radius * 0.12, -hand_angle.sin() * radius * 0.12)
        .line_to(hand_angle.cos() * radius * 0.55, hand_angle.sin() * radius * 0.55)
        .stroke(primary, 6.0, 0.74);
    t.move_to(0.0, 0.0)
        .line_to(second_angle.cos() * radius * 0.65, second_angle.sin() * radius * 0.65)
        .stroke(secondary, 2.0, 0.72);
    t.circle(0.0, 0.0, radius * 0.055).fill(primary, 0.85);
}

/// Folded ribbons (22).
fn draw_folded_ribbons(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    for band in 0..5 {
        let y = (-0.5 + band as f32 * 0.25) * radius;
        let offset = (if band % 2 == 0 { 1.0 } else { -1.0 }) * direction;
        t.move_to(-bleed[0], y)
            .cubic_to(
                -radius * 0.38, y - radius * 0.28 * offset,
                radius * 0.18, y + radius * 0.28 * offset,
                bleed[0], y,
            )
            .stroke(if band % 2 == 0 { primary } else { secondary }, if band == 2 { 12.0 } else { 5.0 }, 0.24 + band as f32 * 0.08);
        t.move_to(-bleed[0], y + radius * 0.055)
            .cubic_to(
                -radius * 0.38, y - radius * 0.28 * offset + radius * 0.055,
                radius * 0.18, y + radius * 0.28 * offset + radius * 0.055,
                bleed[0], y + radius * 0.055,
            )
            .stroke(primary, 1.0, 0.3);
    }
    t.move_to(-radius * 0.34, -bleed[1]).line_to(-radius * 0.34, -radius * 0.18).stroke(primary, 2.0, 0.18);
    t.move_to(radius * 0.34, radius * 0.18).line_to(radius * 0.34, bleed[1]).stroke(primary, 2.0, 0.18);
}

/// Halftone poster (23).
fn draw_halftone_poster(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let spacing = radius * 0.17;
    let bleed = shot_mg_bleed(width, height, radius);
    let columns = ((bleed[0] * 2.0) / spacing).ceil() as i64 + 2;
    let rows = ((bleed[1] * 2.0) / spacing).ceil() as i64 + 2;
    for row in 0..rows {
        for column in 0..columns {
            let x = (column as f32 - (columns as f32 - 1.0) / 2.0) * spacing;
            let y = (row as f32 - (rows as f32 - 1.0) / 2.0) * spacing;
            let distance = x.hypot(y) / radius;
            let pulse = 0.5 + 0.5 * (column as f32 * 0.9 + row as f32 * 1.4 + seed as f32 * 0.08).sin();
            let dot_radius = radius * (0.009 + (0.72 - distance).max(0.0) * 0.034 + pulse * 0.012);
            t.circle(x, y, dot_radius).fill(if (row + column) % 5 == 0 { secondary } else { primary }, 0.24 + pulse * 0.5);
        }
    }
    t.move_to(-bleed[0], -bleed[1] * 0.72).line_to(bleed[0], -bleed[1] * 0.72).stroke(secondary, 2.0, 0.28);
    t.move_to(-bleed[0] * 0.58, -bleed[1]).line_to(-bleed[0] * 0.58, bleed[1]).stroke(primary, 1.0, 0.2);
}

/// Additional variants 18..23; returns true if handled.
pub fn draw_additional_variant(
    t: &mut MgCanvas,
    variant: i64,
    width: f32,
    height: f32,
    radius: f32,
    seed: i64,
    primary: Color,
    secondary: Color,
) -> bool {
    match variant {
        18 => draw_contour_atlas(t, radius, seed, primary, secondary),
        19 => draw_radial_wave(t, radius, seed, primary, secondary),
        20 => draw_transit_blueprint(t, width, height, radius, seed, primary, secondary),
        21 => draw_chronograph(t, radius, seed, primary, secondary),
        22 => draw_folded_ribbons(t, width, height, radius, seed, primary, secondary),
        23 => draw_halftone_poster(t, width, height, radius, seed, primary, secondary),
        _ => return false,
    }
    true
}
