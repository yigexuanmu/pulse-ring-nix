//! Port of folia's themed backgrounds (`sonnetShotMgArchitecture.ts`, `sonnetShotMgBotanical.ts`,
//! `sonnetShotMgFlora.ts`, `sonnetShotMgLandscape.ts`, `sonnetThemedShotMg.ts`, variants 24–35)
//! and the open-frame backgrounds (`sonnetOpenFrameShotMg.ts`, variants 36–47).

use crate::lyricstyles::mg::{draw_leaf, draw_petal, fill_polygon, stroke_polygon, MgCanvas, shot_mg_bleed};
use crate::lyricstyles::mg_geo::Color;

const TAU: f32 = std::f32::consts::TAU;

/// Golden-ratio prime used by folia's `sonnetRandom.ts` hash mix.
const SONNET_GOLDEN: u32 = 0x9E3779B9;

#[inline]
fn mix_mg_themed_seed(seed: u32, salt: u32) -> u32 {
    (seed ^ salt).wrapping_mul(SONNET_GOLDEN)
}

/// Deterministic 0..1 jitter per `(seed, index, salt)`. Port of folia
/// `sonnetHash01` from `sonnetRandom.ts`; identical byte-for-byte to
/// `sonnet_hash01` in `sonnet.rs` (kept duplicated so `mg_themed` stays
/// free of the layout-shaped sonnet module; both must match folia's salt
/// schedule to keep stagger reproducible).
#[inline]
fn mg_themed_hash01(seed: i64, index: u32, salt: u32) -> f32 {
    let s = (seed as u32).wrapping_add(index.wrapping_add(1).wrapping_mul(97));
    mix_mg_themed_seed(s, salt) as f32 / 4294967296.0
}

fn rotate_point(x: f32, y: f32, angle: f32) -> [f32; 2] {
    [
        x * angle.cos() - y * angle.sin(),
        x * angle.sin() + y * angle.cos(),
    ]
}

// ------------------------------------------------------------------ architecture 24..26

fn draw_greenhouse(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    let shell = [
        [-radius * 0.7, radius * 0.58],
        [-radius * 0.7, -radius * 0.12],
        [0.0, -radius * 0.62],
        [radius * 0.7, -radius * 0.12],
        [radius * 0.7, radius * 0.58],
    ];
    fill_polygon(t, &shell, primary, 0.055);
    stroke_polygon(t, &shell, primary, 0.68, 3.0);
    t.move_to(0.0, -radius * 0.62).line_to(0.0, radius * 0.58).stroke(secondary, 2.0, 0.5);
    for pane in -3..=3 {
        let x = pane as f32 * radius * 0.18;
        t.move_to(x, radius * 0.58).line_to(x * 0.38, -radius * (0.58 - (pane as i32).abs() as f32 * 0.04)).stroke(
            if pane % 2 != 0 { secondary } else { primary },
            1.0,
            0.32,
        );
    }
    let door_x = radius * 0.2 * direction;
    t.rect(door_x - radius * 0.13, radius * 0.08, radius * 0.26, radius * 0.5).fill(secondary, 0.1);
    t.rect(door_x - radius * 0.13, radius * 0.08, radius * 0.26, radius * 0.5).stroke(secondary, 2.0, 0.62);
    t.move_to(-bleed[0], radius * 0.58).line_to(-radius * 0.7, radius * 0.58).stroke(primary, 1.0, 0.3);
    t.move_to(radius * 0.7, radius * 0.58).line_to(bleed[0], radius * 0.58).stroke(primary, 1.0, 0.3);
}

fn draw_pagoda(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let lean = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    for floor in 0..4 {
        let y = radius * (0.47 - floor as f32 * 0.27);
        let half_width = radius * (0.5 - floor as f32 * 0.075);
        let roof = [
            [-half_width * 1.18, y],
            [-half_width, y - radius * 0.11],
            [0.0, y - radius * 0.19],
            [half_width, y - radius * 0.11],
            [half_width * 1.18, y],
        ];
        let color = if floor % 2 != 0 { secondary } else { primary };
        fill_polygon(t, &roof, color, 0.07 + floor as f32 * 0.025);
        stroke_polygon(t, &roof, color, 0.58, 2.0);
        t.rect(-half_width * 0.68, y, half_width * 1.36, radius * 0.17).fill(primary, 0.035 + floor as f32 * 0.018);
        t.rect(-half_width * 0.68, y, half_width * 1.36, radius * 0.17).stroke(primary, 1.0, 0.36);
    }
    t.move_to(0.0, -radius * 0.62).line_to(radius * 0.035 * lean, -radius * 0.78).stroke(secondary, 3.0, 0.65);
    t.move_to(-bleed[0], radius * 0.64).line_to(-radius * 0.52, radius * 0.64).stroke(secondary, 1.0, 0.22);
    t.move_to(radius * 0.52, radius * 0.64).line_to(bleed[0], radius * 0.64).stroke(secondary, 1.0, 0.22);
}

fn draw_city_facade(t: &mut MgCanvas, width: f32, height: f32, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let heights = [0.52f32, 0.88, 0.66, 1.08, 0.74, 0.94, 0.58];
    let building_width = radius * 0.205;
    for (index, &height_ratio) in heights.iter().enumerate() {
        let x = radius * (-0.73 + index as f32 * 0.24);
        let height = radius * height_ratio;
        let color = if index % 3 == 1 { secondary } else { primary };
        t.rect(x, radius * 0.62 - height, building_width, height).fill(color, 0.045 + (index % 3) as f32 * 0.035);
        t.rect(x, radius * 0.62 - height, building_width, height).stroke(color, if index == 3 { 3.0 } else { 1.5 }, 0.5);
        for row in 0..(height_ratio * 6.0) as i64 {
            for column in 0..2 {
                if (row + column as i64 + index as i64 + 0) % 3 == 0 {
                    t.rect(
                        x + radius * (0.035 + column as f32 * 0.085),
                        radius * 0.53 - height + row as f32 * radius * 0.13,
                        radius * 0.045,
                        radius * 0.055,
                    ).fill(if column == 1 { secondary } else { primary }, 0.2);
                }
            }
        }
    }
    t.move_to(-bleed[0], radius * 0.63).line_to(bleed[0], radius * 0.63).stroke(primary, 4.0, 0.48);
}

// ------------------------------------------------------------------ botanical 27..29

fn draw_fern(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let tilt = (if seed % 2 != 0 { 1.0 } else { -1.0 }) * 0.18;
    t.move_to(-radius * 0.12, radius * 0.72)
        .cubic_to(-radius * 0.04, radius * 0.2, radius * 0.12, -radius * 0.24, radius * 0.02, -radius * 0.72)
        .stroke(primary, 3.0, 0.62);
    for index in 0..13 {
        let ratio = index as f32 / 13.0;
        let x = -radius * 0.12 + radius * 0.14 * ratio;
        let y = radius * (0.63 - ratio * 1.23);
        let length = radius * (0.3 - (ratio - 0.5).abs() * 0.18);
        let c1 = if index % 3 != 0 { primary } else { secondary };
        let c2 = if index % 3 != 0 { secondary } else { primary };
        draw_leaf(t, x, y, length, length * 0.22, std::f32::consts::PI + tilt - ratio * 0.25, c1, 0.09 + ratio * 0.05);
        draw_leaf(t, x, y, length, length * 0.22, -tilt + ratio * 0.25, c2, 0.07 + ratio * 0.04);
    }
}

fn draw_ginkgo(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    t.move_to(-radius * 0.72 * direction, radius * 0.55)
        .cubic_to(-radius * 0.25 * direction, radius * 0.16, radius * 0.08 * direction, -radius * 0.12, radius * 0.65 * direction, -radius * 0.5)
        .stroke(primary, 5.0, 0.38);
    for index in 0..8 {
        let ratio = index as f32 / 7.0;
        let x = (-0.58 + ratio * 1.12) * radius * direction;
        let y = (0.4 - ratio * 0.78 + (index as f32 * 1.8).sin() * 0.08) * radius;
        let angle = -1.1 + (index % 3) as f32 * 0.7;
        let size = radius * (0.13 + (index % 4) as f32 * 0.018);
        t.move_to(x, y).line_to(x + angle.cos() * size * 0.7, y + angle.sin() * size * 0.7).stroke(secondary, 1.5, 0.42);
        let cx = x + angle.cos() * size;
        let cy = y + angle.sin() * size;
        t.move_to(cx, cy).arc(cx, cy, size, angle + std::f32::consts::PI * 0.1, angle + std::f32::consts::PI * 0.9, false).line_to(cx, cy)
            .fill(if index % 2 != 0 { primary } else { secondary }, 0.09 + (index % 3) as f32 * 0.04);
        t.move_to(cx, cy).arc(cx, cy, size, angle + std::f32::consts::PI * 0.1, angle + std::f32::consts::PI * 0.9, false).line_to(cx, cy)
            .stroke(primary, 1.5, 0.58);
    }
}

fn draw_climbing_vine(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let mirror = if seed % 2 == 0 { 1.0 } else { -1.0 };
    for vine in 0..3 {
        let offset = (vine as f32 - 1.0) * radius * 0.3;
        t.move_to(offset, radius * 0.76)
            .cubic_to(
                offset + radius * 0.5 * mirror, radius * 0.38,
                offset - radius * 0.48 * mirror, -radius * 0.1,
                offset + radius * 0.22 * mirror, -radius * 0.76,
            )
            .stroke(if vine == 1 { secondary } else { primary }, if vine == 1 { 3.0 } else { 1.5 }, 0.46);
        for leaf in 0..5 {
            let ratio = (leaf + 1) as f32 / 6.0;
            let x = offset + (ratio * std::f32::consts::PI * 4.0 + vine as f32).sin() * radius * 0.16;
            let y = radius * (0.7 - ratio * 1.36);
            draw_leaf(t, x, y, radius * 0.2, radius * 0.055, if leaf % 2 != 0 { -0.25 } else { std::f32::consts::PI + 0.25 }, if leaf % 2 != 0 { secondary } else { primary }, 0.08 + vine as f32 * 0.035);
        }
    }
}

// ------------------------------------------------------------------ flora 30..32

fn draw_camellia(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let turn = (seed % 12) as f32 * std::f32::consts::PI / 72.0;
    for ring in 0..3 {
        let count = 7 + ring * 4;
        for index in 0..count {
            let angle = turn + (index as f32 / count as f32) * TAU + ring as f32 * 0.12;
            draw_petal(t, 0.0, 0.0, radius * (0.28 + ring as f32 * 0.15), radius * (0.075 + ring as f32 * 0.018), angle, if ring == 1 { secondary } else { primary }, 0.07 + ring as f32 * 0.045);
        }
    }
    t.circle(0.0, 0.0, radius * 0.1).fill(secondary, 0.22);
    t.circle(0.0, 0.0, radius * 0.13).stroke(primary, 3.0, 0.68);
}

fn draw_tulip_field(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    for index in 0..7 {
        let x = (-0.66 + index as f32 * 0.22) * radius;
        let top = (-0.3 + ((seed + index * 5) % 5) as f32 * 0.085) * radius;
        let bottom = radius * 0.68;
        t.move_to(x, bottom).cubic_to(x + radius * 0.04 * direction, radius * 0.28, x - radius * 0.05 * direction, top + radius * 0.12, x, top)
            .stroke(if index % 2 != 0 { secondary } else { primary }, 2.0, 0.5);
        draw_leaf(t, x, radius * 0.24, radius * 0.26, radius * 0.055, if index % 2 != 0 { -2.7 } else { -0.45 }, primary, 0.1);
        let bloom_color = if index % 3 == 0 { secondary } else { primary };
        t.move_to(x, top + radius * 0.14)
            .quad_to(x - radius * 0.18, top - radius * 0.04, x - radius * 0.11, top - radius * 0.2)
            .line_to(x, top - radius * 0.1)
            .line_to(x + radius * 0.11, top - radius * 0.2)
            .quad_to(x + radius * 0.18, top - radius * 0.04, x, top + radius * 0.14)
            .fill(bloom_color, 0.12 + (index % 3) as f32 * 0.045);
        t.move_to(x, top + radius * 0.14)
            .quad_to(x - radius * 0.18, top - radius * 0.04, x - radius * 0.11, top - radius * 0.2)
            .line_to(x, top - radius * 0.1).line_to(x + radius * 0.11, top - radius * 0.2)
            .quad_to(x + radius * 0.18, top - radius * 0.04, x, top + radius * 0.14)
            .stroke(bloom_color, 2.0, 0.65);
    }
}

fn draw_wildflower(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for stem in 0..9 {
        let x = (-0.72 + stem as f32 * 0.18) * radius;
        let lean = (((seed + stem * 7) % 9) - 4) as f32 * radius * 0.018;
        let flower_y = (-0.45 + ((seed + stem * 3) % 6) as f32 * 0.08) * radius;
        t.move_to(x, radius * 0.72).quad_to(x - lean, radius * 0.12, x + lean, flower_y)
            .stroke(if stem % 2 != 0 { secondary } else { primary }, 1.5, 0.42);
        for petal in 0..5 {
            let angle = (petal as f32 / 5.0) * TAU - std::f32::consts::FRAC_PI_2;
            draw_petal(t, x + lean, flower_y, radius * 0.105, radius * 0.032, angle, if stem % 3 != 0 { primary } else { secondary }, 0.1);
        }
        t.circle(x + lean, flower_y, radius * 0.025).fill(secondary, 0.48);
    }
}

// ------------------------------------------------------------------ landscape 33..35

fn draw_terraces(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    for band in 0..7 {
        let y = radius * (-0.5 + band as f32 * 0.16);
        let amplitude = radius * (0.09 + band as f32 * 0.012);
        t.move_to(-bleed[0], y)
            .cubic_to(-radius * 0.35, y + amplitude * direction, radius * 0.08, y - amplitude * direction, bleed[0], y + amplitude * 0.35)
            .line_to(bleed[0], y + radius * 0.12)
            .cubic_to(radius * 0.12, y + radius * 0.04, -radius * 0.3, y + radius * 0.2, -bleed[0], y + radius * 0.12)
            .fill(if band % 2 != 0 { secondary } else { primary }, 0.025 + band as f32 * 0.018);
        t.move_to(-bleed[0], y)
            .cubic_to(-radius * 0.35, y + amplitude * direction, radius * 0.08, y - amplitude * direction, bleed[0], y + amplitude * 0.35)
            .stroke(if band % 2 != 0 { secondary } else { primary }, if band % 3 == 0 { 2.5 } else { 1.0 }, 0.34 + band as f32 * 0.04);
    }
}

fn draw_mountain_lake(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let shift = (((seed % 7) - 3) as f32 / 3.0) * radius * 0.04;
    let back = [
        [-bleed[0], radius * 0.1], [-radius * 0.42, -radius * 0.46],
        [-radius * 0.14, -radius * 0.16], [radius * 0.22, -radius * 0.62],
        [bleed[0], radius * 0.1],
    ];
    let front = [
        [-bleed[0], radius * 0.22], [-radius * 0.28 + shift, -radius * 0.2],
        [radius * 0.06, radius * 0.05], [radius * 0.48 + shift, -radius * 0.28], [bleed[0], radius * 0.22],
    ];
    fill_polygon(t, &back, secondary, 0.07);
    stroke_polygon(t, &back, secondary, 0.46, 1.5);
    fill_polygon(t, &front, primary, 0.12);
    stroke_polygon(t, &front, primary, 0.66, 2.5);
    for line in 0..7 {
        let y = radius * (0.28 + line as f32 * 0.07);
        let inset = radius * (0.08 + (line % 3) as f32 * 0.08);
        t.move_to(-bleed[0] + inset, y).line_to(bleed[0] - inset, y)
            .stroke(if line % 2 != 0 { secondary } else { primary }, 1.0, 0.2 + line as f32 * 0.035);
    }
    t.circle(-radius * 0.48, -radius * 0.48, radius * 0.1).fill(secondary, 0.14);
    t.circle(-radius * 0.48, -radius * 0.48, radius * 0.13).stroke(secondary, 2.0, 0.5);
}

fn draw_coastal_cliff(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bleed = shot_mg_bleed(width, height, radius);
    let cliff = [
        [-bleed[0] * direction, bleed[1]], [-radius * 0.78 * direction, -radius * 0.1],
        [-radius * 0.5 * direction, -radius * 0.34], [-radius * 0.18 * direction, radius * 0.08],
        [radius * 0.08 * direction, radius * 0.58],
    ];
    fill_polygon(t, &cliff, primary, 0.11);
    stroke_polygon(t, &cliff, primary, 0.62, 2.5);
    let tower_x = -radius * 0.5 * direction;
    t.rect(tower_x - radius * 0.09, -radius * 0.42, radius * 0.18, radius * 0.45).fill(secondary, 0.12);
    t.rect(tower_x - radius * 0.09, -radius * 0.42, radius * 0.18, radius * 0.45).stroke(secondary, 2.0, 0.7);
    t.move_to(tower_x - radius * 0.14, -radius * 0.42).line_to(tower_x, -radius * 0.56).line_to(tower_x + radius * 0.14, -radius * 0.42)
        .fill(secondary, 0.2);
    t.move_to(tower_x - radius * 0.14, -radius * 0.42).line_to(tower_x, -radius * 0.56).line_to(tower_x + radius * 0.14, -radius * 0.42)
        .line_to(tower_x - radius * 0.14, -radius * 0.42)
        .stroke(secondary, 2.0, 0.72);
    for wave in 0..6 {
        let y = radius * (0.14 + wave as f32 * 0.1);
        t.move_to(-radius * 0.05 * direction, y)
            .quad_to(radius * 0.35 * direction, y - radius * 0.08, bleed[0] * direction, y)
            .stroke(if wave % 2 != 0 { secondary } else { primary }, if wave % 3 == 0 { 2.0 } else { 1.0 }, 0.26 + wave as f32 * 0.045);
    }
}

// ------------------------------------------------------------------ celestial 48..57
// Port of folia `sonnetShotMgCelestial.ts` (variants 48..57): ten celestial
// extended backgrounds, all open — no closed viewport frames, no clip masks,
// each motif split into many short stroke/fill commands so the shared
// stagger schedule grows them in layered, offset waves.
// (folia `sonnetShotMgCelestial.ts:1..250`, drawers 48..57.)

// 48: twin log-spiral arms with a bright core and free-floating star dust.
fn draw_spiral_galaxy(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for arm in 0..2 {
        let offset = arm as f32 * std::f32::consts::PI + mg_themed_hash01(seed, arm, 101) * 0.5;
        t.move_to(offset.cos() * radius * 0.06, offset.sin() * radius * 0.05);
        let steps = 56;
        for i in 1..=steps {
            let tt = i as f32 / steps as f32;
            let angle = offset + tt * std::f32::consts::PI * 3.1;
            let r = radius * (0.06 + tt * 0.62);
            t.line_to(angle.cos() * r, angle.sin() * r * 0.72);
        }
        t.stroke(if arm == 0 { primary } else { secondary }, 2.0, 0.5 - arm as f32 * 0.12);
    }
    t.circle(0.0, 0.0, radius * 0.07).fill(primary, 0.7);
    t.circle(0.0, 0.0, radius * 0.12).stroke(primary, 1.0, 0.3);
    for i in 0..14 {
        let angle = mg_themed_hash01(seed, i, 103) * TAU;
        let r = radius * (0.2 + mg_themed_hash01(seed, i, 107) * 0.55);
        t.circle(angle.cos() * r, angle.sin() * r * 0.72, 1.4 + mg_themed_hash01(seed, i, 109) * 2.2)
            .fill(if i % 3 == 0 { secondary } else { primary }, 0.3 + mg_themed_hash01(seed, i, 113) * 0.35);
    }
}

// 49: a comet head with three curved tail trails and cross sparkles.
fn draw_comet_trail(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let hx = radius * 0.34 * direction;
    let hy = -radius * 0.18;
    for tail in 0..3 {
        let spread = (tail as f32 - 1.0) * radius * 0.12;
        t.move_to(hx - direction * radius * 0.04, hy + spread * 0.3)
            .cubic_to(
                hx - direction * radius * 0.35, hy + spread,
                hx - direction * radius * 0.6, hy + radius * 0.16 + spread,
                hx - direction * radius * (0.85 + tail as f32 * 0.06), hy + radius * 0.3 + spread * 1.2,
            )
            .stroke(if tail == 1 { secondary } else { primary }, 3.0 - tail as f32, 0.55 - tail as f32 * 0.12);
    }
    t.circle(hx, hy, radius * 0.09).fill(primary, 0.75);
    t.circle(hx, hy, radius * 0.14).stroke(primary, 1.0, 0.35);
    for i in 0..5 {
        let x = (mg_themed_hash01(seed, i, 127) - 0.5) * radius * 1.4;
        let y = radius * (0.1 + mg_themed_hash01(seed, i, 131) * 0.5);
        let s = 2.5 + mg_themed_hash01(seed, i, 137) * 2.5;
        t.move_to(x - s, y).line_to(x + s, y).stroke(secondary, 1.0, 0.45);
        t.move_to(x, y - s).line_to(x, y + s).stroke(secondary, 1.0, 0.45);
    }
}

// 50: eclipsed disc with an uneven corona of alternating rays.
fn draw_eclipse_corona(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let disc_r = radius * 0.24;
    t.circle(0.0, 0.0, disc_r).fill(primary, 0.16);
    t.circle(0.0, 0.0, disc_r).stroke(secondary, 2.0, 0.65);
    t.circle(0.0, 0.0, disc_r * 1.14).stroke(primary, 1.0, 0.25);
    let rays = 28;
    for i in 0..rays {
        let angle = (i as f32 / rays as f32) * TAU + mg_themed_hash01(seed, i, 139) * 0.08;
        let inner = disc_r * 1.2;
        let outer = radius * (if i % 2 == 0 { 0.6 } else { 0.42 }) * (0.85 + mg_themed_hash01(seed, i, 149) * 0.3);
        t.move_to(angle.cos() * inner, angle.sin() * inner)
            .line_to(angle.cos() * outer, angle.sin() * outer)
            .stroke(if i % 4 == 0 { secondary } else { primary }, if i % 2 == 0 { 2.0 } else { 1.0 }, 0.3 + (i % 3) as f32 * 0.1);
    }
}

// 51: diagonal meteor streaks with glowing heads, all open-ended.
fn draw_meteor_shower(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    for i in 0..8 {
        let x = (mg_themed_hash01(seed, i, 151) - 0.5) * radius * 1.5;
        let y = -radius * 0.55 + mg_themed_hash01(seed, i, 157) * radius * 0.9;
        let len = radius * (0.2 + mg_themed_hash01(seed, i, 163) * 0.3);
        let dx = direction * len;
        let dy = len * 0.55;
        t.move_to(x, y).line_to(x - dx, y - dy).stroke(primary, 2.0, 0.55);
        t.move_to(x - dx * 0.15, y - dy * 0.15 + 3.0)
            .line_to(x - dx * 0.85, y - dy * 0.85 + 3.0)
            .stroke(secondary, 1.0, 0.3);
        t.circle(x, y, 2.0 + mg_themed_hash01(seed, i, 167) * 2.0)
            .fill(if i % 2 == 0 { secondary } else { primary }, 0.7);
    }
}

// 52: broken orbit rings carrying small satellite diamonds.
fn draw_orbit_satellites(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for ring in 0..3 {
        let r = radius * (0.28 + ring as f32 * 0.18);
        let gap_start = mg_themed_hash01(seed, ring, 173) * TAU;
        let segs = 3 + ring;
        for s in 0..segs {
            let start = gap_start + (s as f32 / segs as f32) * TAU;
            t.arc(0.0, 0.0, r, start, start + (TAU / segs as f32) * 0.68, false)
                .stroke(if ring == 1 { secondary } else { primary }, if ring == 0 { 2.0 } else { 1.0 }, 0.35 + ring as f32 * 0.08);
        }
        let sat_angle = mg_themed_hash01(seed, ring, 179) * TAU;
        let sx = sat_angle.cos() * r;
        let sy = sat_angle.sin() * r;
        let d = 5.0 + ring as f32 * 2.0;
        t.move_to(sx, sy - d)
            .line_to(sx + d, sy)
            .line_to(sx, sy + d)
            .line_to(sx - d, sy)
            .line_to(sx, sy - d)
            .fill(secondary, 0.75);
    }
    t.circle(0.0, 0.0, radius * 0.06).fill(primary, 0.8);
}

// 53: vertical aurora ribbons flowing down from the top, no edges.
fn draw_aurora_ribbons(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for band in 0..4 {
        let x0 = -radius * 0.6 + band as f32 * radius * 0.38 + (mg_themed_hash01(seed, band, 181) - 0.5) * radius * 0.1;
        let sway = (if band % 2 == 0 { 1.0 } else { -1.0 }) * radius * 0.2;
        t.move_to(x0, -radius * 0.75)
            .cubic_to(
                x0 + sway, -radius * 0.35,
                x0 - sway, radius * 0.1,
                x0 + sway * 0.6, radius * 0.55,
            )
            .stroke(if band % 2 == 0 { primary } else { secondary }, 7.0 - band as f32, 0.16 + band as f32 * 0.05);
        t.move_to(x0 + radius * 0.06, -radius * 0.7)
            .cubic_to(
                x0 + sway + radius * 0.06, -radius * 0.3,
                x0 - sway + radius * 0.06, radius * 0.12,
                x0 + sway * 0.6 + radius * 0.06, radius * 0.5,
            )
            .stroke(primary, 1.0, 0.3);
    }
    for i in 0..8 {
        t.circle(
            (mg_themed_hash01(seed, i, 191) - 0.5) * radius * 1.5,
            -radius * 0.6 + mg_themed_hash01(seed, i, 193) * radius * 0.5,
            1.4,
        ).fill(secondary, 0.5);
    }
}

// 54: crescent with halo ring and hanging star pendants.
fn draw_crescent_halo(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let moon_r = radius * 0.3;
    let cx = -radius * 0.12;
    let cy = -radius * 0.1;
    // Build crescent path: top to bottom around the right arc, then back up
    // via a quadratic that carves the inner terminator. (folia lines 29-34.)
    t.move_to(cx, cy - moon_r);
    t.arc(cx, cy, moon_r, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, false);
    t.quad_to(cx - moon_r * 0.45, cy, cx, cy - moon_r);
    t.fill(primary, 0.55);
    t.circle(cx, cy, moon_r * 1.35).stroke(secondary, 1.0, 0.3);
    t.circle(cx, cy, moon_r * 1.5).stroke(primary, 1.0, 0.16);
    for i in 0..3 {
        let px = radius * (0.18 + i as f32 * 0.16);
        let top_y = -radius * 0.5 + mg_themed_hash01(seed, i, 197) * radius * 0.1;
        let len = radius * (0.14 + mg_themed_hash01(seed, i, 199) * 0.12);
        t.move_to(px, top_y).line_to(px, top_y + len).stroke(primary, 1.0, 0.4);
        let sr = 4.0 + i as f32;
        let sy = top_y + len + sr;
        t.move_to(px, sy - sr)
            .line_to(px + sr * 0.25, sy - sr * 0.25)
            .line_to(px + sr, sy)
            .line_to(px + sr * 0.25, sy + sr * 0.25)
            .line_to(px, sy + sr)
            .line_to(px - sr * 0.25, sy + sr * 0.25)
            .line_to(px - sr, sy)
            .line_to(px - sr * 0.25, sy - sr * 0.25)
            .line_to(px, sy - sr)
            .stroke(secondary, 1.0, 0.6);
    }
}

// 55: nested organic nebula veils — closed bezier blobs, no straight edges.
// (NB: MgCanvas stroke()/fill() mem::take the path, so for the faint double-
// pass strokes + fills below we rebuild the path on the second pass. visual
// output is identical to folia `moveTo…stroke().fill()` on the same path.)
fn draw_nebula_veil(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for blob in 0..4 {
        let bx = (mg_themed_hash01(seed, blob, 211) - 0.5) * radius * 0.5;
        let by = (mg_themed_hash01(seed, blob, 223) - 0.5) * radius * 0.4;
        let br = radius * (0.2 + blob as f32 * 0.1);
        let wobble = mg_themed_hash01(seed, blob, 227) * 0.6;
        // outline pass
        t.move_to(bx + br, by);
        t.cubic_to(bx + br, by - br * (0.6 + wobble * 0.3), bx + br * 0.5, by - br, bx, by - br * (0.9 - wobble * 0.2));
        t.cubic_to(bx - br * 0.6, by - br * 0.8, bx - br, by - br * 0.3, bx - br * (0.85 + wobble * 0.2), by + br * 0.2);
        t.cubic_to(bx - br * 0.7, by + br * 0.7, bx - br * 0.2, by + br, bx + br * 0.3, by + br * (0.8 + wobble * 0.2));
        t.cubic_to(bx + br * 0.8, by + br * 0.6, bx + br, by + br * 0.4, bx + br, by);
        t.stroke(if blob % 2 == 0 { primary } else { secondary }, 1.5, 0.35 - blob as f32 * 0.04);
        // faint fill for the first two blobs (rebuild path — see note above)
        if blob < 2 {
            t.move_to(bx + br, by);
            t.cubic_to(bx + br, by - br * (0.6 + wobble * 0.3), bx + br * 0.5, by - br, bx, by - br * (0.9 - wobble * 0.2));
            t.cubic_to(bx - br * 0.6, by - br * 0.8, bx - br, by - br * 0.3, bx - br * (0.85 + wobble * 0.2), by + br * 0.2);
            t.cubic_to(bx - br * 0.7, by + br * 0.7, bx - br * 0.2, by + br, bx + br * 0.3, by + br * (0.8 + wobble * 0.2));
            t.cubic_to(bx + br * 0.8, by + br * 0.6, bx + br, by + br * 0.4, bx + br, by);
            t.fill(primary, 0.05);
        }
    }
    for i in 0..10 {
        t.circle(
            (mg_themed_hash01(seed, i, 229) - 0.5) * radius * 1.2,
            (mg_themed_hash01(seed, i, 233) - 0.5) * radius * 1.0,
            1.2 + mg_themed_hash01(seed, i, 239) * 1.8,
        ).fill(primary, 0.25 + mg_themed_hash01(seed, i, 241) * 0.3);
    }
}

// 56: survey-style star map — faint cross grid plus one bright constellation.
fn draw_star_map(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for gx in 0..5 {
        for gy in 0..4 {
            let x = -radius * 0.6 + gx as f32 * radius * 0.3;
            let y = -radius * 0.45 + gy as f32 * radius * 0.3;
            t.move_to(x - 3.0, y).line_to(x + 3.0, y).stroke(primary, 1.0, 0.18);
            t.move_to(x, y - 3.0).line_to(x, y + 3.0).stroke(primary, 1.0, 0.18);
        }
    }
    let nodes = 6;
    let mut px = 0.0_f32;
    let mut py = 0.0_f32;
    for i in 0..nodes {
        let x = -radius * 0.5 + mg_themed_hash01(seed, i, 251) * radius;
        let y = -radius * 0.4 + mg_themed_hash01(seed, i, 257) * radius * 0.8;
        if i > 0 {
            t.move_to(px, py).line_to(x, y).stroke(secondary, 1.5, 0.55);
        }
        t.circle(x, y, 3.0).fill(primary, 0.8);
        t.circle(x, y, 6.5).stroke(primary, 1.0, 0.3);
        px = x;
        py = y;
    }
}

// 57: moon above, open tide arcs below — a bridge between sky and sea.
fn draw_lunar_tide(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let mx = radius * 0.28 * (if seed % 2 == 0 { 1.0 } else { -1.0 });
    let my = -radius * 0.34;
    t.circle(mx, my, radius * 0.16).fill(primary, 0.2);
    t.circle(mx, my, radius * 0.16).stroke(primary, 2.0, 0.6);
    t.arc(mx, my, radius * 0.24, std::f32::consts::PI * 0.2, std::f32::consts::PI * 0.8, false)
        .stroke(secondary, 1.0, 0.35);
    for row in 0..4 {
        let y = radius * (0.05 + row as f32 * 0.14);
        let arcs = 6 - row;
        for i in 0..arcs {
            let x = -radius * 0.62 + i as f32 * radius * 0.24 + (row % 2) as f32 * radius * 0.12;
            t.arc(x, y, radius * 0.09, std::f32::consts::PI, TAU, false)
                .stroke(if row % 2 == 0 { primary } else { secondary }, if row == 0 { 2.0 } else { 1.0 }, 0.45 - row as f32 * 0.07);
        }
    }
}

// ------------------------------------------------------------------ marine 58..67
// Port of folia `sonnetShotMgMarine.ts` (lines 1..282): ten marine
// open-frame compositions (waves, shells, coral, lighthouses, compass,
// sails, bubbles, tide pools, seaweed, currents). All bleed past the
// viewport via `shot_mg_bleed`; every stroke is its own command for the
// staggered-growth schedule. (folia `sonnetShotMgMarine.ts:1..282`.)

// 58: rows of repeating wave-crest arcs across the full bleed width.
fn draw_wave_scrolls(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    for row in 0..4 {
        let y = -radius * 0.3 + row as f32 * radius * 0.22;
        let crest_r = radius * 0.1;
        let step = crest_r * 2.1;
        let count = ((bleed[0] * 2.0) / step).ceil() as i64;
        for i in 0..count {
            let x = -bleed[0] + i as f32 * step + (row % 2) as f32 * crest_r;
            t.arc(x, y, crest_r, std::f32::consts::PI, TAU, false)
                .stroke(if (i + row) % 3 == 0 { secondary } else { primary }, if row == 1 { 2.0 } else { 1.0 }, 0.42 - row as f32 * 0.06);
        }
    }
    for i in 0..6 {
        t.circle(
            (mg_themed_hash01(seed, i, 263) - 0.5) * bleed[0] * 1.4,
            -radius * 0.55 + mg_themed_hash01(seed, i, 269) * radius * 0.25,
            1.6,
        ).fill(secondary, 0.4);
    }
}

// 59: nautilus shell — sampled log spiral with chamber dividers, open outer tip.
fn draw_nautilus(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let turns = 2.6;
    let steps = 90;
    let start_angle = mg_themed_hash01(seed, 0, 271) * TAU;
    t.move_to(start_angle.cos() * radius * 0.04, start_angle.sin() * radius * 0.04);
    for i in 1..=steps {
        let tt = i as f32 / steps as f32;
        let angle = start_angle + tt * turns * TAU;
        let r = radius * (0.04 + tt * 0.5);
        t.line_to(angle.cos() * r, angle.sin() * r * 0.94);
    }
    t.stroke(primary, 2.5, 0.65);
    // Chamber dividers radiate from the spiral core at growing radii.
    for i in 1..=8 {
        let tt = i as f32 / 9.0;
        let angle = start_angle + tt * turns * TAU;
        let r = radius * (0.04 + tt * 0.5);
        t.move_to(angle.cos() * r * 0.55, angle.sin() * r * 0.52)
            .line_to(angle.cos() * r, angle.sin() * r * 0.94)
            .stroke(secondary, 1.0, 0.35);
    }
    t.circle(0.0, 0.0, radius * 0.05).stroke(primary, 1.5, 0.5);
}

// 60: coral branch grown from the bottom with forked limbs and tip buds.
// folia uses a recursive `fork`; we unroll into an explicit stack since
// Rust closures can't borrow `t` mut-recursively without a RefCell. Depth
// is capped at 2 in folia (so exactly 1+2+4 = 7 fork calls). We emit each
// fork's stroke + bud exactly as folia does.
fn draw_coral_branch(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let base_x = (if seed % 2 == 0 { -1.0 } else { 1.0 }) * radius * 0.12;
    // Explicit unroll of folia's recursive `fork(x,y,angle,len,width,depth,limb)`.
    // Depth<2 means: root fork (depth 0) → 2 children (depth 1) → 4 grandchildren (depth 2).
    // 7 forks total, order = root, then left-spine DFS (depth 1 left, depth 2 left, depth 2 right,
    // depth 1 right, depth 2 left, depth 2 right) — matches the JS call order.
    let mut queue: Vec<(f32, f32, f32, f32, f32, i64, i64)> = vec![(base_x, radius * 0.62, -std::f32::consts::FRAC_PI_2, radius * 0.34, 3.0, 0, 0)];
    while let Some((x, y, angle, len, width_, depth, limb)) = queue.pop() {
        let ex = x + angle.cos() * len;
        let ey = y + angle.sin() * len;
        t.move_to(x, y)
            .quad_to(x + (angle + 0.3).cos() * len * 0.5, y + (angle + 0.3).sin() * len * 0.5, ex, ey)
            .stroke(if depth == 0 { secondary } else { primary }, width_, 0.6 - depth as f32 * 0.12);
        t.circle(ex, ey, width_ * 0.9).fill(secondary, 0.5);
        if depth < 2 {
            let spread = 0.55 + mg_themed_hash01(seed, limb as u32, 277) * 0.3;
            // Push children in REVERSE so the DFS pops them in folia's call order
            // (left child = first call = last popped):
            //   fork(ex, ey, angle+spread*0.8, len*0.68, max(1,width-1), depth+1, limb*2+2) ;  // right
            //   fork(ex, ey, angle-spread,    len*0.62, max(1,width-1), depth+1, limb*2+1) ;  // left
            queue.push((ex, ey, angle + spread * 0.8, len * 0.68, (width_ - 1.0).max(1.0), depth + 1, limb * 2 + 2));
            queue.push((ex, ey, angle - spread, len * 0.62, (width_ - 1.0).max(1.0), depth + 1, limb * 2 + 1));
        }
    }
    // A few detached polyps drifting nearby.
    for i in 0..5 {
        t.circle(
            base_x + (mg_themed_hash01(seed, i, 281) - 0.5) * radius * 0.9,
            radius * (0.3 + mg_themed_hash01(seed, i, 283) * 0.3),
            1.8,
        ).fill(primary, 0.35);
    }
}

// 61: lighthouse on a low rock, twin light beams fanning out, open sea arcs.
fn draw_lighthouse_beam(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let bx = -radius * 0.3 * direction;
    let base_y = radius * 0.42;
    // Tower: tapered trapezoid outline, no base bar.
    t.move_to(bx - radius * 0.09, base_y)
        .line_to(bx - radius * 0.05, base_y - radius * 0.42)
        .line_to(bx + radius * 0.05, base_y - radius * 0.42)
        .line_to(bx + radius * 0.09, base_y)
        .stroke(primary, 2.0, 0.6);
    t.rect(bx - radius * 0.07, base_y - radius * 0.52, radius * 0.14, radius * 0.1)
        .stroke(primary, 1.5, 0.55);
    t.circle(bx, base_y - radius * 0.47, radius * 0.025).fill(secondary, 0.9);
    // Twin beams fan to the open right side.
    for beam in 0..2 {
        let spread = radius * (0.1 + beam as f32 * 0.12);
        t.move_to(bx, base_y - radius * 0.47)
            .line_to(bx + direction * radius * 0.85, base_y - radius * 0.47 - spread)
            .stroke(secondary, 1.5, 0.4 - beam as f32 * 0.1);
        t.move_to(bx, base_y - radius * 0.47)
            .line_to(bx + direction * radius * 0.85, base_y - radius * 0.47 + spread)
            .stroke(secondary, 1.5, 0.4 - beam as f32 * 0.1);
    }
    // Sea arcs below, unconnected.
    for i in 0..5 {
        let x = -radius * 0.6 + i as f32 * radius * 0.3;
        t.arc(x, radius * 0.56, radius * 0.1, std::f32::consts::PI, TAU, false)
            .stroke(primary, 1.0, 0.35);
    }
}

// 62: compass rose with long cardinal needles and a partial degree arc.
fn draw_compass_rose(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let rose_r = radius * 0.42;
    for i in 0..8 {
        let angle = (i as f32 / 8.0) * TAU - std::f32::consts::FRAC_PI_2;
        let long = i % 2 == 0;
        let len = if long { rose_r } else { rose_r * 0.55 };
        let half_width = if long { 0.09 } else { 0.06 };
        // Needle = thin triangle from center.
        let tx = angle.cos() * len;
        let ty = angle.sin() * len;
        let lx = (angle + half_width).cos() * rose_r * 0.16;
        let ly = (angle + half_width).sin() * rose_r * 0.16;
        let rx = (angle - half_width).cos() * rose_r * 0.16;
        let ry = (angle - half_width).sin() * rose_r * 0.16;
        t.move_to(lx, ly).line_to(tx, ty).line_to(rx, ry)
            .stroke(if long { primary } else { secondary }, if long { 2.0 } else { 1.0 }, if long { 0.65 } else { 0.45 });
        if long && i % 4 == 0 {
            t.move_to(lx, ly).line_to(tx, ty).line_to(rx, ry).fill(primary, 0.1);
        }
    }
    t.circle(0.0, 0.0, rose_r * 0.14).stroke(primary, 1.5, 0.6);
    t.circle(0.0, 0.0, rose_r * 0.05).fill(secondary, 0.8);
    // Degree ticks only along one open arc — deliberately not a closed dial.
    let arc_start = mg_themed_hash01(seed, 0, 293) * TAU;
    for i in 0..=24 {
        let angle = arc_start + (i as f32 / 24.0) * std::f32::consts::PI * 1.2;
        let inner = rose_r * 1.12;
        let outer = inner + if i % 6 == 0 { 10.0 } else { 5.0 };
        t.move_to(angle.cos() * inner, angle.sin() * inner)
            .line_to(angle.cos() * outer, angle.sin() * outer)
            .stroke(primary, 1.0, 0.4);
    }
}

// 63: three abstract sailboats with curved sails and open water dashes.
fn draw_sail_regatta(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for boat in 0..3 {
        let scale = 1.0 - boat as f32 * 0.24;
        let bx = (boat as f32 - 1.0) * radius * 0.44 + (mg_themed_hash01(seed, boat, 307) - 0.5) * radius * 0.08;
        let by = radius * (0.28 - boat as f32 * 0.12);
        let mast_h = radius * 0.34 * scale;
        t.move_to(bx, by).line_to(bx, by - mast_h).stroke(primary, 2.0, 0.6);
        // Curved sail via quadratic leech.
        t.move_to(bx, by - mast_h)
            .quad_to(bx + radius * 0.2 * scale, by - mast_h * 0.55, bx, by - mast_h * 0.08)
            .stroke(if boat == 1 { secondary } else { primary }, 1.5, 0.55);
        t.move_to(bx, by - mast_h * 0.92)
            .line_to(bx - radius * 0.13 * scale, by - mast_h * 0.1)
            .line_to(bx, by - mast_h * 0.1)
            .stroke(primary, 1.0, 0.4);
        // Hull: shallow arc, open at both ends.
        t.move_to(bx - radius * 0.15 * scale, by)
            .quad_to(bx, by + radius * 0.07 * scale, bx + radius * 0.15 * scale, by)
            .stroke(primary, 2.0, 0.55);
    }
    for i in 0..7 {
        let x = -radius * 0.66 + i as f32 * radius * 0.22;
        let y = radius * (0.42 + (i % 2) as f32 * 0.05);
        t.move_to(x, y).line_to(x + radius * 0.1, y).stroke(secondary, 1.0, 0.35);
    }
}

// 64: three rising bubble columns with highlight arcs on the large ones.
fn draw_bubble_rise(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for column in 0..3 {
        let x = (column as f32 - 1.0) * radius * 0.34 + (mg_themed_hash01(seed, column, 311) - 0.5) * radius * 0.12;
        let count = 5 + column;
        for i in 0..count {
            let tt = i as f32 / count as f32;
            let y = radius * 0.55 - tt * radius * 1.05;
            let wobble = (mg_themed_hash01(seed, column * 10 + i, 313) - 0.5) * radius * 0.08;
            let r = radius * (0.02 + tt * 0.055);
            t.circle(x + wobble, y, r)
                .stroke(if i % 3 == 0 { secondary } else { primary }, 1.0, 0.35 + tt * 0.35);
            if r > radius * 0.05 {
                t.arc(x + wobble, y, r * 0.55, std::f32::consts::PI * 1.1, std::f32::consts::PI * 1.6, false)
                    .stroke(secondary, 1.0, 0.5);
            }
        }
    }
}

// 65: overlapping organic tide-pool rings with pebble dots inside.
fn draw_tide_pools(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for pool in 0..4 {
        let px = (mg_themed_hash01(seed, pool, 317) - 0.5) * radius * 0.7;
        let py = (mg_themed_hash01(seed, pool, 331) - 0.2) * radius * 0.5;
        let pr = radius * (0.16 + mg_themed_hash01(seed, pool, 337) * 0.12);
        t.move_to(px + pr, py);
        t.cubic_to(px + pr, py - pr * 0.7, px + pr * 0.4, py - pr, px, py - pr * 0.9);
        t.cubic_to(px - pr * 0.7, py - pr * 0.8, px - pr, py - pr * 0.2, px - pr * 0.9, py + pr * 0.3);
        t.cubic_to(px - pr * 0.6, py + pr * 0.8, px + pr * 0.2, py + pr, px + pr * 0.6, py + pr * 0.7);
        t.cubic_to(px + pr * 0.95, py + pr * 0.5, px + pr, py + pr * 0.3, px + pr, py);
        t.stroke(if pool % 2 == 0 { primary } else { secondary }, 1.5, 0.45);
        for pebble in 0..3 {
            t.circle(
                px + (mg_themed_hash01(seed, pool * 4 + pebble, 347) - 0.5) * pr,
                py + (mg_themed_hash01(seed, pool * 4 + pebble, 349) - 0.5) * pr * 0.7,
                1.6 + pebble as f32,
            ).fill(secondary, 0.45);
        }
    }
}

// 66: tall seaweed blades swaying from the bottom with drifting air bubbles.
fn draw_seaweed_sway(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for blade in 0..6 {
        let x = -radius * 0.55 + blade as f32 * radius * 0.22 + (mg_themed_hash01(seed, blade, 353) - 0.5) * radius * 0.06;
        let h = radius * (0.4 + mg_themed_hash01(seed, blade, 359) * 0.3);
        let sway = (if blade % 2 == 0 { 1.0 } else { -1.0 }) * radius * 0.12;
        t.move_to(x, radius * 0.62)
            .cubic_to(x + sway, radius * 0.62 - h * 0.4, x - sway, radius * 0.62 - h * 0.7, x + sway * 0.5, radius * 0.62 - h)
            .stroke(if blade % 2 == 0 { primary } else { secondary }, 2.0, 0.5 - blade as f32 * 0.03);
        t.circle(x + sway * 0.5, radius * 0.62 - h, 2.0).fill(secondary, 0.5);
    }
    for i in 0..6 {
        t.circle(
            (mg_themed_hash01(seed, i, 367) - 0.5) * radius * 1.1,
            -radius * 0.5 + mg_themed_hash01(seed, i, 373) * radius * 0.5,
            1.4 + mg_themed_hash01(seed, i, 379) * 1.6,
        ).stroke(primary, 1.0, 0.35);
    }
}

// 67: layered horizontal current lines with scattered fish chevrons.
fn draw_deep_current(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    for line in 0..5 {
        let y = -radius * 0.4 + line as f32 * radius * 0.2;
        let lift = (if line % 2 == 0 { 1.0 } else { -1.0 }) * radius * 0.06;
        t.move_to(-bleed[0], y)
            .cubic_to(-radius * 0.3, y + lift, radius * 0.3, y - lift, bleed[0], y)
            .stroke(if line == 2 { secondary } else { primary }, if line == 2 { 2.0 } else { 1.0 }, 0.32 + (line % 3) as f32 * 0.07);
    }
    // Fish chevrons swim against the current, offset per seed.
    for i in 0..6 {
        let x = (mg_themed_hash01(seed, i, 383) - 0.5) * radius * 1.2;
        let y = -radius * 0.3 + mg_themed_hash01(seed, i, 389) * radius * 0.6;
        let s = 5.0 + mg_themed_hash01(seed, i, 397) * 4.0;
        let flip = if i % 2 == 0 { 1.0 } else { -1.0 };
        t.move_to(x - s * flip, y - s * 0.5).line_to(x, y).line_to(x - s * flip, y + s * 0.5)
            .stroke(secondary, 1.5, 0.55);
    }
}

// ------------------------------------------------------------------ music 68..75
// Port of folia `sonnetShotMgMusic.ts` (lines 1..193): eight music-themed
// open-frame decorations. Staves/ribbons stay open-ended (bleed past the
// viewport via `shot_mg_bleed`); notes, forks, grooves, bars are split into
// short commands for the shared staggered-growth schedule. Every constant
// mirrors folia byte-for-byte, cited `sonnetShotMgMusic.ts:LINE`.

// 68: symmetric waveform mirrored around an open center axis.
// (sonnetShotMgMusic.ts:22-35) `steps=72`, `envelope=sin(pi t)*(0.4+0.6*hash)`,
// `y = mirror*sin(t*TAU*5 + seed*0.13)*radius*0.22*envelope`, mirrored `[-1,+1]`.
fn draw_sound_wave(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let steps = 72;
    let envelope = |tt: f32| (tt * std::f32::consts::PI).sin() * (0.4 + 0.6 * mg_themed_hash01(seed, (tt * 12.0).round() as u32, 401));
    for mirror in [-1.0_f32, 1.0] {
        t.move_to(-bleed[0], 0.0);
        for i in 1..=steps {
            let tt = i as f32 / steps as f32;
            let x = -bleed[0] + tt * bleed[0] * 2.0;
            let y = mirror * (tt * TAU * 5.0 + seed as f32 * 0.13).sin() * radius * 0.22 * envelope(tt);
            t.line_to(x, y);
        }
        t.stroke(if mirror < 0.0 { primary } else { secondary }, if mirror < 0.0 { 2.0 } else { 1.0 }, 0.55);
    }
    t.move_to(-bleed[0], 0.0).line_to(bleed[0], 0.0).stroke(primary, 1.0, 0.18);
}

// 69: vinyl record — 7 grooved arcs each spanning 86% of a turn (always gapped),
// a hub and label dot, plus a tonearm sweeping in from the upper-right pivot.
// (sonnetShotMgMusic.ts:38-52)
fn draw_vinyl_grooves(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for groove in 0..7 {
        let r = radius * (0.2 + groove as f32 * 0.08);
        let gap_at = mg_themed_hash01(seed, groove as u32, 409) * TAU;
        t.arc(0.0, 0.0, r, gap_at, gap_at + TAU * 0.86, false)
            .stroke(
                if groove % 3 == 0 { secondary } else { primary },
                if groove % 3 == 0 { 2.0 } else { 1.0 },
                0.3 + groove as f32 * 0.05,
            );
    }
    t.circle(0.0, 0.0, radius * 0.12).stroke(primary, 2.0, 0.6);
    t.circle(0.0, 0.0, radius * 0.03).fill(secondary, 0.8);
    let pivot_x = radius * 0.62;
    let pivot_y = -radius * 0.52;
    t.circle(pivot_x, pivot_y, radius * 0.035).stroke(primary, 2.0, 0.6);
    t.move_to(pivot_x, pivot_y)
        .line_to(radius * 0.18, -radius * 0.1)
        .stroke(primary, 3.0, 0.5);
    t.circle(radius * 0.18, -radius * 0.1, 3.0).fill(secondary, 0.8);
}

// 70: equalizer bars blooming along a shallow `sin(pi t)` bottom arc — 17 bars,
// each capped with a dot; bars every 4th are secondary. (sonnetShotMgMusic.ts:55-66)
fn draw_equalizer_bloom(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bars = 17;
    for i in 0..bars {
        let tt = i as f32 / (bars - 1) as f32;
        let x = -radius * 0.66 + tt * radius * 1.32;
        let arc_y = radius * 0.5 - (tt * std::f32::consts::PI).sin() * radius * 0.12;
        let h = radius * (0.08 + (tt * std::f32::consts::PI).sin() * 0.3 * (0.5 + mg_themed_hash01(seed, i as u32, 419) * 0.8));
        t.move_to(x, arc_y).line_to(x, arc_y - h)
            .stroke(if i % 4 == 0 { secondary } else { primary }, 3.0, 0.5 + (tt * std::f32::consts::PI).sin() * 0.25);
        t.circle(x, arc_y - h - 4.0, 1.6).fill(secondary, 0.55);
    }
}

// 71: five eighth notes stepping along a `sin(pi*0.9 t)` arc, joined by one
// open beam extending past the last note, with a stray flag off the first stem.
// (sonnetShotMgMusic.ts:69-86)
fn draw_note_arc(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let head_r = radius * 0.045;
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(5);
    for i in 0..5 {
        let tt = i as f32 / 4.0;
        let x = -radius * 0.55 + tt * radius * 1.1;
        let y = radius * 0.22 - (tt * std::f32::consts::PI * 0.9).sin() * radius * 0.34;
        points.push((x, y));
        t.circle(x, y, head_r).fill(if i == 2 { secondary } else { primary }, 0.8);
        t.move_to(x + head_r, y).line_to(x + head_r, y - radius * 0.16)
            .stroke(if i == 2 { secondary } else { primary }, 2.0, 0.65);
    }
    t.move_to(points[0].0 + head_r, points[0].1 - radius * 0.16);
    for i in 1..points.len() {
        t.line_to(points[i].0 + head_r, points[i].1 - radius * 0.16);
    }
    t.line_to(points[4].0 + radius * 0.12, points[4].1 - radius * 0.13);
    t.stroke(primary, 3.0, 0.5);
    t.move_to(points[0].0 + head_r, points[0].1 - radius * 0.16)
        .quad_to(points[0].0 + radius * 0.1, points[0].1 - radius * 0.1, points[0].0 + radius * 0.06, points[0].1 - radius * 0.02)
        .stroke(secondary, 1.5, 0.5);
}

// 72: tuning fork — two prongs, U bend, stem, base weight dot, plus three
// vibration-ring arcs on each side. (sonnetShotMgMusic.ts:89-104)
fn draw_tuning_fork(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let fx = (if seed % 2 == 0 { -1.0 } else { 1.0 }) * radius * 0.08;
    let top_y = -radius * 0.4;
    let prong_w = radius * 0.05;
    let prong_gap = radius * 0.1;
    let u_y = radius * 0.02;
    t.move_to(fx - prong_gap / 2.0 - prong_w, top_y).line_to(fx - prong_gap / 2.0 - prong_w, u_y)
        .stroke(primary, 2.5, 0.65);
    t.move_to(fx + prong_gap / 2.0 + prong_w, top_y).line_to(fx + prong_gap / 2.0 + prong_w, u_y)
        .stroke(primary, 2.5, 0.65);
    t.arc(fx, u_y, prong_gap / 2.0 + prong_w, 0.0, std::f32::consts::PI, false)
        .stroke(primary, 2.5, 0.65);
    t.move_to(fx, u_y + prong_gap / 2.0 + prong_w).line_to(fx, radius * 0.42)
        .stroke(primary, 3.0, 0.6);
    t.circle(fx, radius * 0.46, radius * 0.035).stroke(secondary, 2.0, 0.6);
    for side in [-1.0_f32, 1.0] {
        for ring in 0..3 {
            let r = radius * (0.14 + ring as f32 * 0.1);
            let cx = fx + side * radius * 0.06;
            let cy = top_y + radius * 0.1;
            let (start, end) = if side < 0.0 { (std::f32::consts::PI * 0.6, std::f32::consts::PI * 1.4) } else { (-std::f32::consts::PI * 0.4, std::f32::consts::PI * 0.4) };
            t.arc(cx, cy, r, start, end, false)
                .stroke(if ring == 1 { secondary } else { primary }, 1.5, 0.42 - ring as f32 * 0.1);
        }
    }
}

// 73: piano keys riding a shallow `bezier` ribbon curve, open at both ends;
// black-key indices `1,3,6,8,10` use secondary+thicker. (sonnetShotMgMusic.ts:107-122)
fn draw_piano_ribbon(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    t.move_to(-bleed[0], radius * 0.18)
        .cubic_to(-radius * 0.3, -radius * 0.05, radius * 0.3, radius * 0.3, bleed[0], radius * 0.05)
        .stroke(primary, 1.0, 0.25);
    let keys = 12;
    for i in 0..keys {
        let tt = i as f32 / (keys - 1) as f32;
        let x = -radius * 0.6 + tt * radius * 1.2;
        let base_y = radius * 0.18 + (tt * std::f32::consts::PI).sin() * -radius * 0.1 + tt * -radius * 0.06;
        let black = matches!(i % 12, 1 | 3 | 6 | 8 | 10);
        let len = radius * (if black { 0.14 } else { 0.24 });
        t.move_to(x, base_y).line_to(x, base_y - len)
            .stroke(if black { secondary } else { primary }, if black { 4.0 } else { 3.0 }, if black { 0.7 } else { 0.45 });
    }
}

// 74: metronome — tapered body (base left open by separate strokes), pendulum with
// a weight-rect tip, and three echo arcs sweeping with the pendulum.
// (sonnetShotMgMusic.ts:125-142) `tilt = (hash-0.5)*0.9`.
fn draw_metronome(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let cx = 0.0;
    let base_y = radius * 0.4;
    let top_y = -radius * 0.36;
    t.move_to(cx - radius * 0.2, base_y).line_to(cx - radius * 0.06, top_y).stroke(primary, 2.0, 0.6);
    t.move_to(cx + radius * 0.2, base_y).line_to(cx + radius * 0.06, top_y).stroke(primary, 2.0, 0.6);
    t.move_to(cx - radius * 0.06, top_y).line_to(cx + radius * 0.06, top_y).stroke(primary, 2.0, 0.6);
    let tilt = (mg_themed_hash01(seed, 0, 431) - 0.5) * 0.9;
    let pivot_y = radius * 0.16;
    let tip_x = cx + tilt.sin() * radius * 0.5;
    let tip_y = pivot_y - tilt.cos() * radius * 0.5;
    t.circle(cx, pivot_y, radius * 0.03).fill(secondary, 0.85);
    t.move_to(cx, pivot_y).line_to(tip_x, tip_y).stroke(secondary, 2.0, 0.7);
    t.rect(tip_x - 4.0, tip_y - 4.0, 8.0, 8.0).fill(secondary, 0.7);
    for i in 0..3 {
        let r = radius * (0.24 + i as f32 * 0.12);
        t.arc(cx, pivot_y, r, -std::f32::consts::FRAC_PI_2 - 0.5 - i as f32 * 0.1, -std::f32::consts::FRAC_PI_2 + 0.5 + i as f32 * 0.1, false)
            .stroke(primary, 1.0, 0.3 - i as f32 * 0.06);
    }
}

// 75: five staff lines undulating across the bleed via cubic bezier, plus four
// stepped note heads with stems. `floor(hash*5)` steps the note to a staff slot.
// (sonnetShotMgMusic.ts:145-162)
fn draw_staff_wave(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    for line in 0..5 {
        let y0 = -radius * 0.16 + line as f32 * radius * 0.08;
        let lift = (if line % 2 == 0 { 1.0 } else { -1.0 }) * radius * 0.05;
        t.move_to(-bleed[0], y0)
            .cubic_to(-radius * 0.3, y0 + lift, radius * 0.3, y0 - lift, bleed[0], y0)
            .stroke(primary, 1.0, 0.3 + (if line == 2 { 0.15 } else { 0.0 }));
    }
    for i in 0..4 {
        let x = -radius * 0.45 + i as f32 * radius * 0.3 + (mg_themed_hash01(seed, i as u32, 439) - 0.5) * radius * 0.08;
        let y = -radius * 0.16 + (mg_themed_hash01(seed, i as u32, 443) * 5.0).floor() * radius * 0.08;
        t.circle(x, y, radius * 0.032).fill(if i % 2 == 0 { secondary } else { primary }, 0.85);
        t.move_to(x + radius * 0.032, y).line_to(x + radius * 0.032, y - radius * 0.14)
            .stroke(if i % 2 == 0 { secondary } else { primary }, 1.5, 0.6);
    }
}

// ------------------------------------------------------------------ craft 76..85
// Port of folia `sonnetShotMgCraft.ts` (lines 1..257): ten craft-themed
// open-frame motifs (paper, textile, knot). Weaves and knots use gaps
// instead of masks to suggest over/under crossings — no clip rects.
// Every constant mirrors folia byte-for-byte, cited `sonnetShotMgCraft.ts:LINE`.

// 76: faceted origami crane in line art with two lightly filled folds.
// (sonnetShotMgCraft.ts:14-32) `direction = seed%2 ? 1 : -1`, body diamond +
// raised wing (one filled fold) + neck/head + tail + two crease lines.
fn draw_origami_crane(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let s = radius * 0.34;
    // Body diamond.
    t.move_to(0.0, -s * 0.3).line_to(direction * s * 0.5, 0.0).line_to(0.0, s * 0.35).line_to(-direction * s * 0.5, 0.0)
        .line_to(0.0, -s * 0.3)
        .stroke(primary, 2.0, 0.6);
    // Raised wing (stroke outline + light fill).
    t.move_to(0.0, -s * 0.3).line_to(-direction * s * 0.15, -s * 0.95).line_to(direction * s * 0.28, -s * 0.1)
        .stroke(primary, 1.5, 0.5);
    t.move_to(0.0, -s * 0.3).line_to(-direction * s * 0.15, -s * 0.95).line_to(-direction * s * 0.42, s * 0.02)
        .fill(primary, 0.07);
    // Neck + head.
    t.move_to(direction * s * 0.5, 0.0)
        .line_to(direction * s * 0.78, -s * 0.62)
        .line_to(direction * s * 0.98, -s * 0.5)
        .stroke(secondary, 1.5, 0.6);
    // Tail.
    t.move_to(-direction * s * 0.5, 0.0).line_to(-direction * s * 0.85, -s * 0.5)
        .stroke(primary, 1.5, 0.5);
    // Crease lines.
    t.move_to(0.0, -s * 0.3).line_to(0.0, s * 0.35).stroke(secondary, 1.0, 0.3);
    t.move_to(-direction * s * 0.5, 0.0).line_to(direction * s * 0.5, 0.0)
        .stroke(secondary, 1.0, 0.3);
}

// 77: paper plane with a segmented looping trail behind it. Three arc segments
// with deliberate gaps. (sonnetShotMgCraft.ts:35-51)
fn draw_paper_plane_trail(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let direction = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let px = radius * 0.4 * direction;
    let py = -radius * 0.28;
    let s = radius * 0.16;
    t.move_to(px + direction * s, py).line_to(px - direction * s * 0.8, py - s * 0.55).line_to(px - direction * s * 0.35, py)
        .line_to(px + direction * s, py)
        .stroke(primary, 2.0, 0.65);
    t.move_to(px + direction * s, py).line_to(px - direction * s * 0.35, py).line_to(px - direction * s * 0.8, py + s * 0.4)
        .stroke(secondary, 1.5, 0.5);
    for seg in 0..3 {
        let start = std::f32::consts::PI * (0.1 + seg as f32 * 0.55);
        t.arc(px - direction * radius * 0.35, py + radius * 0.3, radius * (0.34 + seg as f32 * 0.06), start, start + std::f32::consts::PI * 0.4, false)
            .stroke(if seg == 1 { secondary } else { primary }, 1.5, 0.45 - seg as f32 * 0.08);
    }
}

// 78: woven band — vertical strips pass over/under two horizontals via gaps
// (no clip masks). `(i+row)%2==0` chooses the gapped vs continuous pattern.
// (sonnetShotMgCraft.ts:54-76)
fn draw_weave_band(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let band_y = [-radius * 0.12, radius * 0.12];
    let strips = 7;
    // Horizontal strips first (behind), broken at every other crossing.
    for (row, &y) in band_y.iter().enumerate() {
        for i in 0..strips {
            let x0 = -radius * 0.63 + i as f32 * radius * 0.18;
            if (i + row) % 2 == 0 {
                t.move_to(x0 + radius * 0.02, y).line_to(x0 + radius * 0.16, y)
                    .stroke(if row == 0 { primary } else { secondary }, 5.0, 0.4);
            } else {
                t.move_to(x0 - radius * 0.05, y).line_to(x0 + radius * 0.02, y)
                    .stroke(if row == 0 { primary } else { secondary }, 5.0, 0.4);
                t.move_to(x0 + radius * 0.16, y).line_to(x0 + radius * 0.23, y)
                    .stroke(if row == 0 { primary } else { secondary }, 5.0, 0.4);
            }
        }
    }
    // Vertical strips on top at the gapped crossings.
    for i in 0..strips {
        let x = -radius * 0.54 + i as f32 * radius * 0.18;
        let over_row = i % 2;
        let y = band_y[over_row];
        t.move_to(x, y - radius * 0.05).line_to(x, y + radius * 0.05)
            .stroke(primary, 6.0, 0.6);
        t.move_to(x, band_y[1 - over_row] - radius * 0.03).line_to(x, band_y[1 - over_row] + radius * 0.03)
            .stroke(secondary, 2.0, 0.3);
    }
}

// 79: figure-eight knot drawn in segments with crossing gaps for over/under.
// Each loop = 2 bezier segments broken at the over point. (sonnetShotMgCraft.ts:79-95)
fn draw_knot_loop(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let s = radius * 0.34;
    // Left loop, broken where the right loop passes over.
    t.move_to(0.0, 0.0);
    t.cubic_to(-s * 0.9, -s * 0.9, -s * 1.5, -s * 0.2, -s * 0.8, s * 0.28);
    t.stroke(primary, 3.0, 0.6);
    t.move_to(-s * 0.62, s * 0.34);
    t.cubic_to(-s * 0.3, s * 0.44, -s * 0.12, s * 0.2, 0.0, 0.0);
    t.stroke(primary, 3.0, 0.6);
    // Right loop, broken where the left loop passes over.
    t.move_to(s * 0.12, -s * 0.08);
    t.cubic_to(s * 0.6, -s * 0.6, s * 1.4, -s * 0.3, s * 0.9, s * 0.2);
    t.stroke(secondary, 3.0, 0.6);
    t.move_to(s * 0.72, s * 0.26);
    t.cubic_to(s * 0.4, s * 0.4, s * 0.05, s * 0.14, -s * 0.06, s * 0.04);
    t.stroke(secondary, 3.0, 0.6);
    // Loose ends drifting out.
    t.move_to(0.0, 0.0).cubic_to(-s * 0.2, s * 0.5, -s * 0.4, s * 0.8, -s * 0.3, s * 1.1)
        .stroke(primary, 2.0, 0.4);
    t.move_to(s * 0.06, -s * 0.02).cubic_to(s * 0.3, -s * 0.5, s * 0.5, -s * 0.8, s * 0.42, -s * 1.05)
        .stroke(secondary, 2.0, 0.4);
}

// 80: cross-stitch sampler rows that fade toward the edges. `count = 7 - |row - 1.5|`
// (folia uses float arithmetic; we keep semantic parity by computing the float then truncating).
// (sonnetShotMgCraft.ts:98-112)
fn draw_stitch_sampler(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let stitch = |t: &mut MgCanvas, x: f32, y: f32, size: f32, color: Color, alpha: f32| {
        t.move_to(x - size, y - size).line_to(x + size, y + size).stroke(color, 1.5, alpha);
        t.move_to(x + size, y - size).line_to(x - size, y + size).stroke(color, 1.5, alpha);
    };
    for row in 0..4 {
        let y = -radius * 0.36 + row as f32 * radius * 0.24;
        // folia `count = 7 - Math.abs(row - 1.5)` — float math, JS truncates via
        // `for (let i=0;i<count;i++)` boundary. Rust loop range stays integral;
        // floor() of the float matches the JS loop's behavior.
        let count = (7.0 - (row as f32 - 1.5).abs()).floor() as i64;
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * radius * 0.16 + (row % 2) as f32 * radius * 0.08;
            let edge_fade = 1.0 - (i as f32 - (count as f32 - 1.0) / 2.0).abs() / (count as f32 / 2.0 + 0.5);
            stitch(t, x, y, 4.0 + (row % 2) as f32, if (i + row) % 3 == 0 { secondary } else { primary }, 0.25 + edge_fade * 0.4);
        }
    }
}

// 81: folded fan — ribs from a pivot, double guard arc, open at the top.
// (sonnetShotMgCraft.ts:115-129) `spread = PI*0.9`, 11 ribs across `[-spread/2, +spread/2]`
// around `-PI/2`, each rib length = `radius * (0.5 + sin(i/(ribs-1)*PI)*0.12)`.
fn draw_folded_fan(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let pivot_y = radius * 0.42;
    let ribs = 11;
    let spread = std::f32::consts::PI * 0.9;
    for i in 0..ribs {
        let angle = -std::f32::consts::FRAC_PI_2 - spread / 2.0 + (i as f32 / (ribs - 1) as f32) * spread;
        let len = radius * (0.5 + (i as f32 / (ribs - 1) as f32 * std::f32::consts::PI).sin() * 0.12);
        t.move_to(0.0, pivot_y)
            .line_to(angle.cos() * len, pivot_y + angle.sin() * len)
            .stroke(if i % 2 == 0 { primary } else { secondary }, if i == 5 { 2.5 } else { 1.5 }, 0.5);
    }
    t.arc(0.0, pivot_y, radius * 0.5, -std::f32::consts::FRAC_PI_2 - spread / 2.0, -std::f32::consts::FRAC_PI_2 + spread / 2.0, false)
        .stroke(primary, 2.0, 0.45);
    t.arc(0.0, pivot_y, radius * 0.58, -std::f32::consts::FRAC_PI_2 - spread / 2.0 + 0.06, -std::f32::consts::FRAC_PI_2 + spread / 2.0 - 0.06, false)
        .stroke(secondary, 1.0, 0.3);
    t.circle(0.0, pivot_y, radius * 0.03).fill(secondary, 0.8);
}

// 82: curling gift ribbon — sampled spiral (2.4 turns, 64 steps) with a
// parallel echo stroke at +radius*0.035 offset, plus a loose end flick.
// (sonnetShotMgCraft.ts:132-151)
fn draw_ribbon_curl(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let cx = radius * 0.15 * (if seed % 2 == 0 { 1.0 } else { -1.0 });
    let start = mg_themed_hash01(seed, 0, 449) * TAU;
    for echo in 0..2 {
        let offset = echo as f32 * radius * 0.035;
        t.move_to(cx + start.cos() * radius * 0.06, -radius * 0.1 + start.sin() * radius * 0.06 + offset);
        let steps = 64;
        for i in 1..=steps {
            let tt = i as f32 / steps as f32;
            let angle = start + tt * TAU * 2.4;
            let r = radius * (0.06 + tt * 0.42);
            t.line_to(cx + angle.cos() * r, -radius * 0.1 + angle.sin() * r * 0.8 + offset);
        }
        t.stroke(if echo == 0 { primary } else { secondary }, if echo == 0 { 3.0 } else { 1.0 }, if echo == 0 { 0.55 } else { 0.3 });
    }
    let end_angle = start + TAU * 2.4;
    let ex = cx + end_angle.cos() * radius * 0.48;
    let ey = -radius * 0.1 + end_angle.sin() * radius * 0.38;
    t.move_to(ex, ey).quad_to(ex + radius * 0.1, ey - radius * 0.12, ex + radius * 0.16, ey - radius * 0.04)
        .stroke(primary, 2.0, 0.5);
}

// 83: three overlapping patchwork triangles with hand-built inner stripes
// (no mask; stripe `half = s*0.55 * (up ? t : 1-t)` computed inline).
// (sonnetShotMgCraft.ts:154-174)
fn draw_patchwork_trio(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let configs = [
        (-radius * 0.22, -radius * 0.05, radius * 0.3, true),
        (radius * 0.18, -radius * 0.12, radius * 0.24, false),
        (radius * 0.05, radius * 0.2, radius * 0.2, true),
    ];
    for (index, &(x, y, s, up)) in configs.iter().enumerate() {
        let top_y = if up { y - s * 0.6 } else { y };
        let base_y = if up { y + s * 0.4 } else { y + s };
        t.move_to(x, top_y).line_to(x + s * 0.55, base_y).line_to(x - s * 0.55, base_y).line_to(x, top_y)
            .stroke(if index == 1 { secondary } else { primary }, 2.0, 0.55);
        for stripe in 1..=3 {
            let tt = stripe as f32 / 4.0;
            let sy = top_y + (base_y - top_y) * tt;
            let half = s * 0.55 * (if up { tt } else { 1.0 - tt });
            t.move_to(x - half, sy).line_to(x + half, sy)
                .stroke(if index == 1 { primary } else { secondary }, 1.0, 0.35);
        }
    }
}

// 84: dreamcatcher — ring, radial web (8 strands) to an off-center hub,
// three hanging strings with feather barbs; bottom stays open.
// (sonnetShotMgCraft.ts:177-200)
fn draw_dreamcatcher(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let ring_r = radius * 0.32;
    let cy = -radius * 0.12;
    t.circle(0.0, cy, ring_r).stroke(primary, 2.0, 0.6);
    let hub_x = radius * 0.05;
    let hub_y = cy - radius * 0.03;
    for i in 0..8 {
        let angle = (i as f32 / 8.0) * TAU + 0.2;
        let rim_x = angle.cos() * ring_r * 0.92;
        let rim_y = cy + angle.sin() * ring_r * 0.92;
        t.move_to(rim_x, rim_y).line_to(hub_x, hub_y)
            .stroke(if i % 2 == 0 { primary } else { secondary }, 1.0, 0.4);
    }
    t.circle(hub_x, hub_y, radius * 0.035).stroke(secondary, 1.5, 0.6);
    for i in -1..=1 {
        let sx = i as f32 * ring_r * 0.55;
        let top_y = cy + (ring_r * ring_r - sx * sx).max(0.0).sqrt();
        let len = radius * (0.22 + (1.0 - (i as f32).abs()) * 0.12 + mg_themed_hash01(seed, (i + 1) as u32, 457) * 0.06);
        t.move_to(sx, top_y).line_to(sx, top_y + len).stroke(primary, 1.0, 0.45);
        let feather_y = top_y + len;
        t.move_to(sx, feather_y).line_to(sx, feather_y + radius * 0.12)
            .stroke(secondary, 1.5, 0.55);
        for barb in 1..=3 {
            let by = feather_y + barb as f32 * radius * 0.03;
            let bl = radius * 0.04 * (1.0 - barb as f32 * 0.18);
            t.move_to(sx, by).line_to(sx - bl, by + radius * 0.02)
                .stroke(secondary, 1.0, 0.4);
            t.move_to(sx, by).line_to(sx + bl, by + radius * 0.02)
                .stroke(secondary, 1.0, 0.4);
        }
    }
}

// 85: tassel curtain — 7 staggered hanging threads with bead tips from a short bar.
// (sonnetShotMgCraft.ts:203-219) `sway = (hash-0.5)*radius*0.08`, `len = radius*(0.3+hash*0.35)`.
fn draw_tassel_drop(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bar_y = -radius * 0.4;
    t.move_to(-radius * 0.3, bar_y).line_to(radius * 0.3, bar_y)
        .stroke(primary, 2.0, 0.4);
    let threads = 7;
    for i in 0..threads {
        let x = -radius * 0.27 + i as f32 * radius * 0.09;
        let len = radius * (0.3 + mg_themed_hash01(seed, i as u32, 461) * 0.35);
        let sway = (mg_themed_hash01(seed, i as u32, 463) - 0.5) * radius * 0.08;
        t.move_to(x, bar_y)
            .quad_to(x + sway, bar_y + len * 0.6, x + sway * 0.6, bar_y + len)
            .stroke(if i % 2 == 0 { primary } else { secondary }, 1.5, 0.45);
        t.circle(x + sway * 0.6, bar_y + len + 3.0, 2.5)
            .fill(if i % 3 == 0 { secondary } else { primary }, 0.6);
    }
}

// 86: pendulum wave — strings and bobs at staggered phases along an arc.
fn draw_pendulum_wave(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let pivot_y = -radius * 0.42;
    let bobs = 9;
    for i in 0..bobs {
        let x = -radius * 0.5 + (i as f32 / (bobs - 1) as f32) * radius;
        let len = radius * (0.4 + i as f32 * 0.035);
        let swing = (i as f32 * 0.9 + seed as f32 * 0.07).sin() * 0.35;
        let bx = x + swing.sin() * len;
        let by_u = pivot_y + swing.cos() * len;
        t.move_to(x, pivot_y).line_to(bx, by_u)
            .stroke(primary, 1.0, 0.4);
        t.circle(bx, by_u, 3.5 + (i % 3) as f32)
            .fill(if i % 2 == 0 { secondary } else { primary }, 0.7);
    }
    t.move_to(-radius * 0.58, pivot_y).line_to(radius * 0.58, pivot_y)
        .stroke(primary, 2.0, 0.35);
}

// 87: dominos toppling along an arc, each rotated a step further.
fn draw_domino_arc(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let count = 10;
    let arc_r = radius * 0.55;
    for i in 0..count {
        let angle = std::f32::consts::PI * 1.15 + (i as f32 / (count - 1) as f32) * std::f32::consts::PI * 0.7;
        let bx = angle.cos() * arc_r;
        let by = angle.sin() * arc_r + radius * 0.5;
        let tilt = (i as f32 / (count - 1) as f32) * 1.1 * (if seed % 2 == 0 { 1.0 } else { -1.0 });
        let w = radius * 0.035;
        let h = radius * 0.14;
        let cos = tilt.cos();
        let sin = tilt.sin();
        let local = [(-w, 0.0), (w, 0.0), (w, -2.0 * h), (-w, -2.0 * h)];
        let corners = [
            (bx + local[0].0 * cos - local[0].1 * sin, by + local[0].0 * sin + local[0].1 * cos),
            (bx + local[1].0 * cos - local[1].1 * sin, by + local[1].0 * sin + local[1].1 * cos),
            (bx + local[2].0 * cos - local[2].1 * sin, by + local[2].0 * sin + local[2].1 * cos),
            (bx + local[3].0 * cos - local[3].1 * sin, by + local[3].0 * sin + local[3].1 * cos),
        ];
        t.move_to(corners[0].0, corners[0].1)
            .line_to(corners[1].0, corners[1].1)
            .line_to(corners[2].0, corners[2].1)
            .line_to(corners[3].0, corners[3].1)
            .line_to(corners[0].0, corners[0].1)
            .stroke(if i % 3 == 0 { secondary } else { primary }, 1.5, 0.55);
    }
}

// 88: three intermeshed gears built from rings and radial teeth.
fn draw_gear_cluster(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let gears = [
        (0.0_f32, 0.0_f32, radius * 0.26, 10),
        (radius * 0.42, -radius * 0.2, radius * 0.16, 8),
        (-radius * 0.4, radius * 0.22, radius * 0.13, 7),
    ];
    for (gi, &(gx, gy, gr, teeth)) in gears.iter().enumerate() {
        let color = if gi == 1 { secondary } else { primary };
        t.circle(gx, gy, gr).stroke(color, 2.0, 0.55);
        t.circle(gx, gy, gr * 0.3).stroke(color, 1.5, 0.45);
        let offset = mg_themed_hash01(seed, gi as u32, 467) * TAU;
        for tooth in 0..teeth {
            let angle = offset + (tooth as f32 / teeth as f32) * TAU;
            t.move_to(gx + angle.cos() * gr, gy + angle.sin() * gr)
                .line_to(gx + angle.cos() * gr * 1.18, gy + angle.sin() * gr * 1.18)
                .stroke(color, 3.0, 0.5);
        }
    }
}

// 89: circuit traces with 45-degree bends and node pads, no board outline.
fn draw_circuit_delta(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let lanes = 4;
    for lane in 0..lanes {
        let y = -radius * 0.36 + lane as f32 * radius * 0.24;
        let bend_x = -radius * 0.3 + mg_themed_hash01(seed, lane as u32, 479) * radius * 0.6;
        let drop_v = (if lane % 2 == 0 { 1.0 } else { -1.0 }) * radius * 0.08;
        t.move_to(-bleed[0], y)
            .line_to(bend_x - drop_v.abs(), y)
            .line_to(bend_x, y + drop_v)
            .line_to(bleed[0], y + drop_v)
            .stroke(if lane == 1 { secondary } else { primary }, 1.5, 0.45);
        t.circle(bend_x, y + drop_v, 3.0).fill(secondary, 0.7);
        t.circle(-bleed[0] * 0.55, y, 2.5).stroke(primary, 1.0, 0.5);
    }
}

// 90: signal tower mast with radiating wave arcs on both sides.
fn draw_signal_tower(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let base_y = radius * 0.45;
    let top_y = -radius * 0.3;
    t.move_to(-radius * 0.12, base_y).line_to(0.0, top_y).line_to(radius * 0.12, base_y)
        .stroke(primary, 2.0, 0.6);
    for brace in 1..=3 {
        let y = base_y - (base_y - top_y) * (brace as f32 / 4.0);
        let half = radius * 0.12 * (1.0 - brace as f32 / 4.5);
        t.move_to(-half, y).line_to(half, y - radius * 0.06)
            .stroke(primary, 1.0, 0.4);
    }
    t.circle(0.0, top_y, radius * 0.03).fill(secondary, 0.9);
    for side in [-1.0_f32, 1.0_f32] {
        for ring in 0..3 {
            let r = radius * (0.12 + ring as f32 * 0.13);
            let (start, end) = if side < 0.0 { (std::f32::consts::PI * 0.75, std::f32::consts::PI * 1.25) } else { (-std::f32::consts::PI * 0.25, std::f32::consts::PI * 0.25) };
            t.arc(0.0, top_y, r, start, end, false)
                .stroke(if ring == 1 { secondary } else { primary }, 1.5, 0.45 - ring as f32 * 0.1);
        }
    }
}

// 91: spiral staircase ascending as staggered tread/riser polylines.
fn draw_spiral_stair(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let steps = 12;
    let start_angle = mg_themed_hash01(seed, 0, 487) * TAU;
    for i in 0..steps {
        let angle = start_angle + i as f32 * 0.42;
        let r = radius * (0.14 + i as f32 * 0.04);
        let x = angle.cos() * r;
        let y = radius * 0.4 - i as f32 * radius * 0.055;
        let tread = radius * 0.09;
        let tx = x + angle.cos() * tread;
        let ty = y + angle.sin() * tread * 0.4;
        t.move_to(x, y)
            .line_to(tx, ty)
            .stroke(if i % 3 == 0 { secondary } else { primary }, 2.0, 0.55);
        t.move_to(tx, ty)
            .line_to(tx, ty - radius * 0.055)
            .stroke(primary, 1.0, 0.35);
    }
    t.move_to(0.0, radius * 0.45).line_to(0.0, -radius * 0.35)
        .stroke(primary, 2.0, 0.3);
}

// 92: falling vertical streams of staggered length with splash arcs below.
fn draw_waterfall_lines(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let streams = 9;
    for i in 0..streams {
        let x = -radius * 0.5 + (i as f32 / (streams - 1) as f32) * radius + (mg_themed_hash01(seed, i as u32, 491) - 0.5) * radius * 0.05;
        let top_y = -radius * 0.6 + mg_themed_hash01(seed, i as u32, 499) * radius * 0.15;
        let len = radius * (0.55 + mg_themed_hash01(seed, i as u32, 503) * 0.35);
        t.move_to(x, top_y).line_to(x, top_y + len)
            .stroke(if i % 3 == 0 { secondary } else { primary }, if i % 3 == 0 { 2.0 } else { 1.0 }, 0.4 + (i % 3) as f32 * 0.08);
    }
    for i in 0..5 {
        let x = -radius * 0.4 + i as f32 * radius * 0.2;
        t.arc(x, radius * 0.5, radius * 0.07, std::f32::consts::PI, TAU, false)
            .stroke(secondary, 1.0, 0.4);
    }
}

// 93: four-blade pinwheel with curved sails around a hub.
fn draw_pinwheel(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let hub_r = radius * 0.05;
    for blade in 0..4 {
        let angle = (blade as f32 / 4.0) * TAU + mg_themed_hash01(seed, 0, 509) * 0.5;
        let tip_r = radius * 0.5;
        let tx = angle.cos() * tip_r;
        let ty = angle.sin() * tip_r;
        let edge_angle = angle + 0.7;
        t.move_to(angle.cos() * hub_r, angle.sin() * hub_r)
            .quad_to(edge_angle.cos() * tip_r * 0.55, edge_angle.sin() * tip_r * 0.55, tx, ty)
            .stroke(if blade % 2 == 0 { primary } else { secondary }, 2.0, 0.55);
        t.move_to(tx, ty)
            .line_to((angle + 0.45).cos() * tip_r * 0.62, (angle + 0.45).sin() * tip_r * 0.62)
            .stroke(if blade % 2 == 0 { primary } else { secondary }, 1.5, 0.4);
    }
    t.circle(0.0, 0.0, hub_r).fill(secondary, 0.8);
    t.circle(0.0, 0.0, radius * 0.56).stroke(primary, 1.0, 0.15);
}

// 94: falling drop above broken ripple arcs — nothing touches the edges.
fn draw_ripple_drop(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let dx = (mg_themed_hash01(seed, 0, 521) - 0.5) * radius * 0.2;
    t.move_to(dx, -radius * 0.52);
    t.cubic_to(dx + radius * 0.07, -radius * 0.36, dx + radius * 0.06, -radius * 0.3, dx, -radius * 0.27);
    t.cubic_to(dx - radius * 0.06, -radius * 0.3, dx - radius * 0.07, -radius * 0.36, dx, -radius * 0.52);
    t.stroke(secondary, 2.0, 0.65);
    for ring in 0..4 {
        let r = radius * (0.14 + ring as f32 * 0.13);
        let y = radius * 0.25;
        let gap_at = mg_themed_hash01(seed, ring as u32, 523) * TAU;
        t.arc(dx, y, r, gap_at, gap_at + TAU * 0.72, false)
            .stroke(if ring % 2 == 0 { primary } else { secondary }, if ring == 0 { 2.0 } else { 1.0 }, 0.5 - ring as f32 * 0.09);
    }
    t.move_to(dx - radius * 0.05, radius * 0.2).line_to(dx - radius * 0.02, radius * 0.12)
        .stroke(primary, 1.5, 0.5);
    t.move_to(dx + radius * 0.05, radius * 0.2).line_to(dx + radius * 0.02, radius * 0.12)
        .stroke(primary, 1.5, 0.5);
}

// 95: suspension bridge — sagging main cable, two towers, open deck line.
fn draw_suspension_bridge(t: &mut MgCanvas, width: f32, height: f32, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let deck_y = radius * 0.3;
    let tower_x = radius * 0.34;
    let tower_top = -radius * 0.28;
    t.move_to(-bleed[0], deck_y).line_to(bleed[0], deck_y).stroke(primary, 2.0, 0.5);
    for side in [-1.0_f32, 1.0_f32] {
        let x = side * tower_x;
        t.move_to(x - radius * 0.03, deck_y).line_to(x - radius * 0.03, tower_top).stroke(primary, 2.0, 0.55);
        t.move_to(x + radius * 0.03, deck_y).line_to(x + radius * 0.03, tower_top).stroke(primary, 2.0, 0.55);
        t.move_to(x - radius * 0.04, tower_top + radius * 0.08).line_to(x + radius * 0.04, tower_top + radius * 0.08).stroke(secondary, 1.5, 0.45);
    }
    t.move_to(-bleed[0], deck_y - radius * 0.1)
        .quad_to(-tower_x, tower_top - radius * 0.06, -tower_x, tower_top)
        .stroke(secondary, 1.5, 0.5);
    t.move_to(-tower_x, tower_top)
        .quad_to(0.0, deck_y - radius * 0.04, tower_x, tower_top)
        .stroke(secondary, 1.5, 0.5);
    t.move_to(tower_x, tower_top)
        .quad_to(bleed[0], deck_y - radius * 0.1, bleed[0], deck_y - radius * 0.06)
        .stroke(secondary, 1.5, 0.5);
    for i in 1..7 {
        let tt = i as f32 / 7.0;
        let x = -tower_x + tt * tower_x * 2.0;
        let cable_y = (1.0 - tt) * (1.0 - tt) * tower_top + 2.0 * (1.0 - tt) * tt * (deck_y - radius * 0.04) + tt * tt * tower_top;
        t.move_to(x, cable_y).line_to(x, deck_y).stroke(primary, 1.0, 0.35);
    }
}

// 96: magnetic field loops through two poles, mirrored top and bottom.
fn draw_field_lines(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let pole_gap = radius * 0.2;
    for lp in 0..4 {
        let bulge = radius * (0.2 + lp as f32 * 0.16);
        for mirror in [-1.0_f32, 1.0_f32] {
            t.move_to(0.0, -pole_gap);
            t.cubic_to(mirror * bulge, -pole_gap - radius * 0.1, mirror * bulge, pole_gap + radius * 0.1, 0.0, pole_gap);
            t.stroke(if lp % 2 == 0 { primary } else { secondary }, if lp == 0 { 2.0 } else { 1.0 }, 0.5 - lp as f32 * 0.08);
        }
    }
    t.circle(0.0, -pole_gap, radius * 0.04).fill(secondary, 0.85);
    t.circle(0.0, pole_gap, radius * 0.04).fill(primary, 0.85);
    t.move_to(-radius * 0.1, -pole_gap).line_to(radius * 0.1, -pole_gap).stroke(secondary, 1.5, 0.5);
    t.move_to(-radius * 0.1, pole_gap).line_to(radius * 0.1, pole_gap).stroke(primary, 1.5, 0.5);
}

// 97: prism splitting one inbound beam into a fanned spectrum.
fn draw_prism_beam(t: &mut MgCanvas, width: f32, height: f32, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    let bleed = shot_mg_bleed(width, height, radius);
    let s = radius * 0.24;
    let top_y = -s * 0.7;
    let base_y = s * 0.55;
    t.move_to(0.0, top_y).line_to(s * 0.8, base_y).line_to(-s * 0.8, base_y).line_to(0.0, top_y)
        .stroke(primary, 2.0, 0.6);
    t.move_to(0.0, top_y).line_to(s * 0.8, base_y).line_to(-s * 0.8, base_y).line_to(0.0, top_y)
        .fill(primary, 0.06);
    let entry_x = -s * 0.35;
    let entry_y = s * 0.05;
    t.move_to(-bleed[0], entry_y + radius * 0.12).line_to(entry_x, entry_y)
        .stroke(secondary, 2.5, 0.6);
    for ray in 0..4 {
        let exit_y = -radius * 0.1 + ray as f32 * radius * 0.09;
        t.move_to(s * 0.4, entry_y - radius * 0.05)
            .line_to(bleed[0], exit_y)
            .stroke(if ray == 1 { secondary } else { primary }, 1.5, 0.45 - ray as f32 * 0.05);
    }
}

// 98: echo arcs bouncing between two unseen walls, offset per hop.
fn draw_echo_arcs(t: &mut MgCanvas, radius: f32, _seed: i64, primary: Color, secondary: Color) {
    for hop in 0..5 {
        let side = if hop % 2 == 0 { -1.0_f32 } else { 1.0_f32 };
        let cx = side * radius * 0.52;
        let cy = -radius * 0.35 + hop as f32 * radius * 0.18;
        let r = radius * (0.14 + hop as f32 * 0.045);
        let (start, end) = if side < 0.0 { (-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2) } else { (std::f32::consts::FRAC_PI_2, std::f32::consts::PI * 1.5) };
        t.arc(cx, cy, r, start, end, false)
            .stroke(if hop % 2 == 0 { primary } else { secondary }, 2.0 - hop as f32 * 0.2, 0.55 - hop as f32 * 0.07);
        t.circle(cx + (if side < 0.0 { r } else { -r }) * 0.4, cy + r * 0.6, 1.8)
            .fill(secondary, 0.5);
    }
}

// 99: diamond kite with cross spars, tail bows and a long free string.
fn draw_kite_string(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let kx = radius * 0.22 * (if seed % 2 == 0 { 1.0 } else { -1.0 });
    let ky = -radius * 0.3;
    let kw = radius * 0.16;
    let kh = radius * 0.22;
    t.move_to(kx, ky - kh).line_to(kx + kw, ky).line_to(kx, ky + kh).line_to(kx - kw, ky).line_to(kx, ky - kh)
        .stroke(primary, 2.0, 0.6);
    t.move_to(kx, ky - kh).line_to(kx, ky + kh).stroke(secondary, 1.0, 0.4);
    t.move_to(kx - kw, ky).line_to(kx + kw, ky).stroke(secondary, 1.0, 0.4);
    t.move_to(kx, ky - kh).line_to(kx + kw, ky).line_to(kx, ky + kh).line_to(kx - kw, ky).line_to(kx, ky - kh)
        .fill(primary, 0.06);
    let mut bows: Vec<(f32, f32)> = Vec::new();
    t.move_to(kx, ky + kh);
    let string_steps = 24;
    for i in 1..=string_steps {
        let tt = i as f32 / string_steps as f32;
        let x = kx - tt * radius * 0.5 + (tt * std::f32::consts::PI * 2.2).sin() * radius * 0.08;
        let y = ky + kh + tt * radius * 0.6;
        t.line_to(x, y);
        if i == 7 || i == 13 || i == 19 {
            bows.push((x, y));
        }
    }
    t.stroke(primary, 1.5, 0.5);
    for (bx, by) in bows {
        let bow_s = radius * 0.035;
        t.move_to(bx, by).line_to(bx - bow_s, by - bow_s).line_to(bx, by - bow_s * 0.3).line_to(bx + bow_s, by - bow_s)
            .line_to(bx, by)
            .stroke(secondary, 1.0, 0.55);
    }
}

/// Themed variants 24..35 + 48..99; returns true if handled.
pub fn draw_themed_variant(
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
        24 => draw_camellia(t, radius, seed, primary, secondary),
        25 => draw_tulip_field(t, radius, seed, primary, secondary),
        26 => draw_wildflower(t, radius, seed, primary, secondary),
        27 => draw_fern(t, radius, seed, primary, secondary),
        28 => draw_ginkgo(t, radius, seed, primary, secondary),
        29 => draw_climbing_vine(t, radius, seed, primary, secondary),
        30 => draw_greenhouse(t, width, height, radius, seed, primary, secondary),
        31 => draw_pagoda(t, width, height, radius, seed, primary, secondary),
        32 => draw_city_facade(t, width, height, radius, seed, primary, secondary),
        33 => draw_terraces(t, width, height, radius, seed, primary, secondary),
        34 => draw_mountain_lake(t, width, height, radius, seed, primary, secondary),
        35 => draw_coastal_cliff(t, width, height, radius, seed, primary, secondary),
        48 => draw_spiral_galaxy(t, radius, seed, primary, secondary),
        49 => draw_comet_trail(t, radius, seed, primary, secondary),
        50 => draw_eclipse_corona(t, radius, seed, primary, secondary),
        51 => draw_meteor_shower(t, radius, seed, primary, secondary),
        52 => draw_orbit_satellites(t, radius, seed, primary, secondary),
        53 => draw_aurora_ribbons(t, radius, seed, primary, secondary),
        54 => draw_crescent_halo(t, radius, seed, primary, secondary),
        55 => draw_nebula_veil(t, radius, seed, primary, secondary),
        56 => draw_star_map(t, radius, seed, primary, secondary),
        57 => draw_lunar_tide(t, radius, seed, primary, secondary),
        58 => draw_wave_scrolls(t, width, height, radius, seed, primary, secondary),
        59 => draw_nautilus(t, radius, seed, primary, secondary),
        60 => draw_coral_branch(t, radius, seed, primary, secondary),
        61 => draw_lighthouse_beam(t, radius, seed, primary, secondary),
        62 => draw_compass_rose(t, radius, seed, primary, secondary),
        63 => draw_sail_regatta(t, radius, seed, primary, secondary),
        64 => draw_bubble_rise(t, radius, seed, primary, secondary),
        65 => draw_tide_pools(t, radius, seed, primary, secondary),
        66 => draw_seaweed_sway(t, radius, seed, primary, secondary),
        67 => draw_deep_current(t, width, height, radius, seed, primary, secondary),
        68 => draw_sound_wave(t, width, height, radius, seed, primary, secondary),
        69 => draw_vinyl_grooves(t, radius, seed, primary, secondary),
        70 => draw_equalizer_bloom(t, radius, seed, primary, secondary),
        71 => draw_note_arc(t, radius, seed, primary, secondary),
        72 => draw_tuning_fork(t, radius, seed, primary, secondary),
        73 => draw_piano_ribbon(t, width, height, radius, seed, primary, secondary),
        74 => draw_metronome(t, radius, seed, primary, secondary),
        75 => draw_staff_wave(t, width, height, radius, seed, primary, secondary),
        76 => draw_origami_crane(t, radius, seed, primary, secondary),
        77 => draw_paper_plane_trail(t, radius, seed, primary, secondary),
        78 => draw_weave_band(t, radius, seed, primary, secondary),
        79 => draw_knot_loop(t, radius, seed, primary, secondary),
        80 => draw_stitch_sampler(t, radius, seed, primary, secondary),
        81 => draw_folded_fan(t, radius, seed, primary, secondary),
        82 => draw_ribbon_curl(t, radius, seed, primary, secondary),
        83 => draw_patchwork_trio(t, radius, seed, primary, secondary),
        84 => draw_dreamcatcher(t, radius, seed, primary, secondary),
        85 => draw_tassel_drop(t, radius, seed, primary, secondary),
        _ => return false,
    }
    true
}

// ------------------------------------------------------------------ open frames 36..47

fn draw_open_arc_brackets(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let bx = width * 0.34;
    let by = height * 0.34;
    let arc_r = radius * 0.17;
    let corners: [[f32; 4]; 4] = [
        [-bx, -by, std::f32::consts::PI, std::f32::consts::PI * 1.5],
        [bx, -by, -std::f32::consts::FRAC_PI_2, 0.0],
        [bx, by, 0.0, std::f32::consts::FRAC_PI_2],
        [-bx, by, std::f32::consts::FRAC_PI_2, std::f32::consts::PI],
    ];
    for (index, c) in corners.iter().enumerate() {
        t.arc(c[0], c[1], arc_r, c[2], c[3], false).stroke(if index % 2 != 0 { secondary } else { primary }, 3.0, 0.6);
        t.arc(c[0], c[1], arc_r * 0.72, c[2], c[3], false).stroke(primary, 1.0, 0.3);
        t.move_to(c[0] - 5.0, c[1]).line_to(c[0] + 5.0, c[1]).stroke(primary, 1.0, 0.5);
        t.move_to(c[0], c[1] - 5.0).line_to(c[0], c[1] + 5.0).stroke(primary, 1.0, 0.5);
    }
    let dot_angle = (seed % 8) as f32 * TAU / 8.0;
    t.circle(dot_angle.cos() * radius * 0.5, dot_angle.sin() * radius * 0.5, radius * 0.02).fill(secondary, 0.7);
}

fn draw_dashed_orbits(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for ring in 0..3 {
        let ring_radius = radius * (0.32 + ring as f32 * 0.18);
        let dashes = 12 + ring * 4;
        let offset = seed as f32 * 0.13 + ring as f32 * 0.7;
        let span = (TAU / dashes as f32) * 0.55;
        for dash in 0..dashes {
            let start = offset + (dash as f32 / dashes as f32) * TAU;
            t.arc(0.0, 0.0, ring_radius, start, start + span, false)
                .stroke(if ring == 1 { secondary } else { primary }, if ring == 1 { 1.0 } else { 2.0 }, 0.28 + ring as f32 * 0.06);
        }
    }
    let marker_angle = seed as f32 * 0.31;
    t.circle(marker_angle.cos() * radius * 0.68, marker_angle.sin() * radius * 0.68, radius * 0.022).fill(primary, 0.75);
    t.circle((marker_angle + std::f32::consts::PI).cos() * radius * 0.5, (marker_angle + std::f32::consts::PI).sin() * radius * 0.5, radius * 0.016).fill(secondary, 0.6);
    t.circle(0.0, 0.0, radius * 0.05).stroke(primary, 1.5, 0.5);
}

fn draw_open_fragments(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for index in 0..5 {
        let angle = (index as f32 / 5.0) * TAU + seed as f32 * 0.05;
        let distance = radius * (0.42 + ((seed + index * 7) % 30) as f32 / 100.0);
        let cx = angle.cos() * distance;
        let cy = angle.sin() * distance * 0.8;
        let size = radius * (0.07 + ((seed + index * 13) % 20) as f32 / 200.0);
        let rotation = seed as f32 * 0.11 + index as f32 * 0.9;
        let p0 = rotate_point(-size, -size, rotation);
        let p1 = rotate_point(size, -size, rotation);
        let p2 = rotate_point(size, size, rotation);
        let p3 = rotate_point(-size, size, rotation);
        t.move_to(cx + p0[0], cy + p0[1]);
        for p in [p1, p2, p3] {
            t.line_to(cx + p[0], cy + p[1]);
        }
        t.stroke(if index % 2 != 0 { secondary } else { primary }, 2.0, 0.5);
        if index as i64 == seed % 5 {
            t.circle(cx, cy, size * 0.28).fill(primary, 0.35);
        }
    }
}

fn draw_horizon_bundles(t: &mut MgCanvas, width: f32, height: f32, seed: i64, primary: Color, secondary: Color) {
    let bundles: [(f32, f32); 2] = [(-height * 0.28, 1.0), (height * 0.3, -1.0)];
    for (bundle_index, (base_y, drift)) in bundles.iter().enumerate() {
        for line in 0..3 {
            let y = base_y + line as f32 * 10.0 * drift;
            let break_at = width * ((((seed + line * 17 + bundle_index as i64 * 31) % 40) + 30) as f32 / 100.0 - 0.5);
            let gap_half = width * 0.045;
            t.move_to(-width * 0.4, y).line_to(break_at - gap_half, y)
                .stroke(if line == 1 { secondary } else { primary }, if line == 1 { 2.0 } else { 1.0 }, 0.42 - line as f32 * 0.08);
            t.move_to(break_at + gap_half, y).line_to(width * 0.4, y)
                .stroke(if line == 1 { secondary } else { primary }, if line == 1 { 2.0 } else { 1.0 }, 0.42 - line as f32 * 0.08);
        }
        t.move_to(-width * 0.42, *base_y).line_to(-width * 0.42 + 6.0, *base_y).stroke(primary, 1.0, 0.4);
    }
    t.rect(width * 0.36, -height * 0.28 - 3.0, width * 0.04, 6.0).fill(secondary, 0.5);
}

fn draw_semi_wreath(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let opening = (seed % 4) as f32 * (std::f32::consts::FRAC_PI_2) + std::f32::consts::FRAC_PI_8;
    let span = TAU * 0.75;
    let outer = radius * 0.58;
    let inner = radius * 0.5;
    t.arc(0.0, 0.0, outer, opening, opening + span, false).stroke(primary, 2.5, 0.55);
    t.arc(0.0, 0.0, inner, opening, opening + span, false).stroke(secondary, 1.0, 0.3);
    let ticks = 18;
    for index in 0..ticks {
        let angle = opening + (index as f32 / ticks as f32) * span;
        t.move_to(angle.cos() * outer, angle.sin() * outer)
            .line_to(angle.cos() * (outer + radius * 0.05), angle.sin() * (outer + radius * 0.05))
            .stroke(primary, 1.0, 0.4);
        if index % 6 == 0 {
            t.circle(angle.cos() * inner, angle.sin() * inner, radius * 0.016).fill(secondary, 0.6);
        }
    }
}

fn draw_side_rulers(t: &mut MgCanvas, width: f32, height: f32, seed: i64, primary: Color, secondary: Color) {
    for &side in &[-1.0f32, 1.0] {
        let x = side * width * 0.36;
        t.move_to(x, -height * 0.3).line_to(x, height * 0.3).stroke(primary, 1.0, 0.35);
        for tick in 0..12 {
            let y = -height * 0.3 + (tick as f32 / 11.0) * height * 0.6;
            let length = if tick % 4 == 0 { 18.0 } else { 9.0 };
            t.move_to(x, y).line_to(x - side * length, y)
                .stroke(if tick % 4 == 0 { secondary } else { primary }, 1.0, 0.45);
        }
        let accent_y = -height * 0.3 + (((seed + if side > 0.0 { 5 } else { 0 }) % 11) as f32 / 11.0) * height * 0.6;
        t.rect(if side > 0.0 { x } else { x - 4.0 }, accent_y - 4.0, 4.0, 8.0).fill(secondary, 0.6);
    }
}

fn draw_diagonal_stream(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let angle = std::f32::consts::FRAC_PI_4 + (seed % 3) as f32 * (std::f32::consts::PI / 12.0);
    let dir_x = angle.cos();
    let dir_y = angle.sin();
    let normal_x = -dir_y;
    let normal_y = dir_x;
    for line in 0..7 {
        let offset = (line as f32 - 3.0) * radius * 0.16;
        let length = radius * (0.55 + ((seed + line * 29) % 40) as f32 / 100.0);
        let cx = normal_x * offset;
        let cy = normal_y * offset;
        t.move_to(cx - dir_x * length, cy - dir_y * length)
            .line_to(cx + dir_x * length * 0.6, cy + dir_y * length * 0.6)
            .stroke(if line == 3 { secondary } else { primary }, if line == 3 { 2.0 } else { 1.0 }, 0.22 + line as f32 * 0.03);
    }
    for (index, &offset) in [-radius * 0.16f32, radius * 0.16].iter().enumerate() {
        let cx = normal_x * offset + dir_x * radius * 0.1;
        let cy = normal_y * offset + dir_y * radius * 0.1;
        let size = radius * 0.07;
        let p0 = rotate_point(0.0, -size, angle);
        let p1 = rotate_point(size, 0.0, angle);
        let p2 = rotate_point(0.0, size, angle);
        let p3 = rotate_point(-size, 0.0, angle);
        t.move_to(cx + p0[0], cy + p0[1]);
        for p in [p1, p2, p3] {
            t.line_to(cx + p[0], cy + p[1]);
        }
        t.stroke(primary, 1.5, 0.55);
        if index == 0 {
            t.circle(cx, cy, size * 0.22).fill(secondary, 0.55);
        }
    }
}

fn draw_corner_petal_spray(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let sign = if seed % 2 == 0 { 1.0 } else { -1.0 };
    let corners: [(f32, f32, f32); 2] = [
        (-sign * width * 0.28, -height * 0.26, (1.0f32).atan2(sign)),
        (sign * width * 0.28, height * 0.26, (-1.0f32).atan2(-sign)),
    ];
    for (corner_index, (cxx, cyy, base)) in corners.iter().enumerate() {
        for petal in 0..5 {
            let spread = (petal as f32 - 2.0) * 0.3;
            let angle = base + spread;
            let length = radius * (0.3 - (petal as f32 - 2.0).abs() * 0.045);
            let end_x = cxx + angle.cos() * length;
            let end_y = cyy + angle.sin() * length;
            let ctrl_x = cxx + (angle + 0.35).cos() * length * 0.55;
            let ctrl_y = cyy + (angle + 0.35).sin() * length * 0.55;
            t.move_to(*cxx, *cyy)
                .quad_to(ctrl_x, ctrl_y, end_x, end_y)
                .stroke(if petal == 2 { secondary } else { primary }, if petal == 2 { 2.0 } else { 1.0 }, 0.4);
        }
        if corner_index == 0 {
            t.circle(*cxx, *cyy, radius * 0.02).fill(primary, 0.6);
        }
    }
}

fn draw_dotted_windows(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let columns = 7;
    let rows = 5;
    for row in 0..rows {
        for column in 0..columns {
            if (row * columns + column + seed) % 5 == 0 {
                continue;
            }
            let x = (column as f32 - (columns as f32 - 1.0) / 2.0) * width * 0.11;
            let y = (row as f32 - (rows as f32 - 1.0) / 2.0) * height * 0.14;
            t.circle(x, y, 1.5).fill(primary, 0.35);
        }
    }
    let arm = radius * 0.1;
    for window in 0..3 {
        let cell_x = (((seed + window * 2) % columns as i64) - (columns as i64 - 1) / 2) as f32 * width * 0.11;
        let cell_y = (((seed + window * 3 + 1) % rows as i64) - (rows as i64 - 1) / 2) as f32 * height * 0.14;
        let flip_x = if (seed + window) % 2 == 0 { 1.0 } else { -1.0 };
        let flip_y = if (seed + window * 2) % 2 == 0 { 1.0 } else { -1.0 };
        t.move_to(cell_x + flip_x * arm, cell_y)
            .line_to(cell_x, cell_y)
            .line_to(cell_x, cell_y + flip_y * arm)
            .stroke(secondary, 2.0, 0.6);
    }
}

fn draw_open_radar(t: &mut MgCanvas, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let span = TAU * 0.56;
    for ring in 0..3 {
        let ring_radius = radius * (0.3 + ring as f32 * 0.18);
        let start = seed as f32 * 0.1 + ring as f32 * 0.9;
        t.arc(0.0, 0.0, ring_radius, start, start + span, false)
            .stroke(if ring == 1 { secondary } else { primary }, if ring == 1 { 2.0 } else { 1.0 }, 0.35 + ring as f32 * 0.06);
    }
    let sweep = seed as f32 * 0.07;
    t.move_to(0.0, 0.0).line_to(sweep.cos() * radius * 0.66, sweep.sin() * radius * 0.66).stroke(primary, 1.5, 0.5);
    let lock_angle = sweep + 0.8;
    let lock_x = lock_angle.cos() * radius * 0.48;
    let lock_y = lock_angle.sin() * radius * 0.48;
    let tick = radius * 0.04;
    t.move_to(lock_x - tick * 2.0, lock_y - tick).line_to(lock_x - tick * 2.0, lock_y - tick * 2.0).line_to(lock_x - tick, lock_y - tick * 2.0)
        .stroke(secondary, 1.5, 0.7);
    t.move_to(lock_x + tick * 2.0, lock_y + tick).line_to(lock_x + tick * 2.0, lock_y + tick * 2.0).line_to(lock_x + tick, lock_y + tick * 2.0)
        .stroke(secondary, 1.5, 0.7);
    t.circle(0.0, 0.0, radius * 0.018).fill(primary, 0.8);
    t.circle(lock_x, lock_y, radius * 0.014).fill(secondary, 0.7);
}

fn draw_brush_strokes(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    for stroke in 0..4 {
        let horizontal = stroke % 2 == 0;
        let along = (((seed + stroke * 23) % 50) as f32 / 100.0 - 0.25) * if horizontal { width } else { height };
        let across = (((seed + stroke * 41) % 60) as f32 / 100.0 - 0.3) * if horizontal { height } else { width };
        let length = radius * (0.3 + ((seed + stroke * 7) % 25) as f32 / 100.0);
        let (x1, y1, x2, y2) = if horizontal {
            (along - length, across, along + length, across)
        } else {
            (across, along - length, across, along + length)
        };
        t.move_to(x1, y1).line_to(x2, y2)
            .stroke(if stroke == 1 { secondary } else { primary }, radius * 0.04, 0.22);
        t.move_to(x1, y1 + if horizontal { radius * 0.035 } else { 0.0 })
            .line_to(if horizontal { x2 * 0.6 } else { x2 }, if horizontal { y2 + radius * 0.035 } else { y2 * 0.6 })
            .stroke(primary, radius * 0.012, 0.45);
    }
    let size = radius * 0.05;
    let anchor_x = width * 0.3;
    let anchor_y = -height * 0.3;
    t.move_to(anchor_x - size, anchor_y - size).line_to(anchor_x + size, anchor_y - size).line_to(anchor_x + size, anchor_y + size)
        .stroke(secondary, 1.5, 0.6);
    t.rect(-anchor_x - size / 2.0, -anchor_y - size / 2.0, size, size).fill(primary, 0.4);
}

fn draw_stitch_corners(t: &mut MgCanvas, width: f32, height: f32, radius: f32, seed: i64, primary: Color, secondary: Color) {
    let arm = radius * 0.18;
    let dash = radius * 0.03;
    let corners: [[f32; 4]; 4] = [
        [-width * 0.39, -height * 0.36, 1.0, 1.0],
        [width * 0.39, -height * 0.36, -1.0, 1.0],
        [width * 0.39, height * 0.36, -1.0, -1.0],
        [-width * 0.39, height * 0.36, 1.0, -1.0],
    ];
    for (index, c) in corners.iter().enumerate() {
        let mut offset = 0.0f32;
        while offset + dash <= arm {
            t.move_to(c[0] + c[2] * offset, c[1]).line_to(c[0] + c[2] * (offset + dash), c[1])
                .stroke(if index % 2 != 0 { secondary } else { primary }, 2.0, 0.5);
            t.move_to(c[0], c[1] + c[3] * offset).line_to(c[0], c[1] + c[3] * (offset + dash))
                .stroke(if index % 2 != 0 { secondary } else { primary }, 2.0, 0.5);
            offset += dash * 2.0;
        }
    }
    let cross_arm = radius * 0.09;
    let mut offset = -cross_arm;
    while offset + dash * 0.6 <= cross_arm {
        t.move_to(offset, 0.0).line_to(offset + dash * 0.6, 0.0).stroke(primary, 1.0, 0.4);
        t.move_to(0.0, offset).line_to(0.0, offset + dash * 0.6).stroke(primary, 1.0, 0.4);
        offset += dash * 1.2;
    }
    t.circle(0.0, 0.0, radius * 0.012).fill(secondary, 0.7);
    let accent = corners[(seed % 4) as usize];
    t.circle(accent[0] + accent[2] * arm, accent[1], radius * 0.014).fill(primary, 0.6);
}

/// Open-frame variants 36..47; returns true if handled.
pub fn draw_open_variant(
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
        36 => draw_open_arc_brackets(t, width, height, radius, seed, primary, secondary),
        37 => draw_dashed_orbits(t, radius, seed, primary, secondary),
        38 => draw_open_fragments(t, radius, seed, primary, secondary),
        39 => draw_horizon_bundles(t, width, height, seed, primary, secondary),
        40 => draw_semi_wreath(t, radius, seed, primary, secondary),
        41 => draw_side_rulers(t, width, height, seed, primary, secondary),
        42 => draw_diagonal_stream(t, radius, seed, primary, secondary),
        43 => draw_corner_petal_spray(t, width, height, radius, seed, primary, secondary),
        44 => draw_dotted_windows(t, width, height, radius, seed, primary, secondary),
        45 => draw_open_radar(t, radius, seed, primary, secondary),
        46 => draw_brush_strokes(t, width, height, radius, seed, primary, secondary),
        47 => draw_stitch_corners(t, width, height, radius, seed, primary, secondary),
        _ => return false,
    }
    true
}

/// Routes variants 24..47 (themed then open-frame); returns false for anything outside.
pub fn draw_variant_dispatch(
    t: &mut MgCanvas,
    variant: i64,
    width: f32,
    height: f32,
    radius: f32,
    seed: i64,
    primary: Color,
    secondary: Color,
) -> bool {
    if draw_themed_variant(t, variant, width, height, radius, seed, primary, secondary) {
        return true;
    }
    draw_open_variant(t, variant, width, height, radius, seed, primary, secondary)
}
