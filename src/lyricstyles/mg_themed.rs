//! Port of folia's themed backgrounds (`sonnetShotMgArchitecture.ts`, `sonnetShotMgBotanical.ts`,
//! `sonnetShotMgFlora.ts`, `sonnetShotMgLandscape.ts`, `sonnetThemedShotMg.ts`, variants 24–35)
//! and the open-frame backgrounds (`sonnetOpenFrameShotMg.ts`, variants 36–47).

use crate::lyricstyles::mg::{draw_leaf, draw_petal, fill_polygon, stroke_polygon, MgCanvas, shot_mg_bleed};
use crate::lyricstyles::mg_geo::Color;

const TAU: f32 = std::f32::consts::TAU;

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

/// Themed variants 24..35; returns true if handled.
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
