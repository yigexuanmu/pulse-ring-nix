//! Folia sonnet v2 — `sonnetShotFlowLayouts.ts` (557 lines) 1:1 port.
//!
//! Flow-based placement passes for the non-poster shot kinds. Each variant
//! keeps its own composition identity (ribbon, cross, orbit, badge...), but
//! all of them share the poster-blocks principles: exact measured boxes, gaps
//! in the `clamp(baseFontSize*0.35, 16, 40)` range, a scan order that equals the
//! timeline order, and uniform global-scale retries instead of per-word shrink
//! loops.

use crate::lyricstyles::sonnet_v2::types::SonnetLayoutDirection;

#[derive(Debug, Clone)]
pub struct SonnetFlowLayoutBox {
    pub index: usize,
    pub is_hero: bool,
    pub is_semi_hero: bool,
    pub display_text: String,
    pub font_scale: f64,
    pub measured_width: f64,
    pub measured_height: f64,
    pub vertical: bool,
    pub layout_direction: SonnetLayoutDirection,
    pub rotation: f64,
    pub x: f64,
    pub y: f64,
    pub enter_x: f64,
    pub enter_y: f64,
}

#[derive(Debug, Clone)]
pub struct SonnetFlowLayoutContext {
    pub boxes: Vec<SonnetFlowLayoutBox>,
    pub hero_index: usize,
    pub width: f64,
    pub height: f64,
    pub flow_gap: f64,
    pub stack_gap: f64,
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.min(max).max(min)
}

/// `resolveSonnetFlowGaps` — word-to-word gap (flowGap) and line/column gap
/// (stackGap) shared by all branches.
pub fn resolve_sonnet_flow_gaps(base_font_size: f64) -> (f64, f64) {
    let flow_gap: f64 = clamp(base_font_size * 0.35, 16.0, 40.0);
    (flow_gap, 24.0_f64.max(flow_gap * 1.35))
}

/// `placeWithGlobalFit` — runs a placement pass at shrinking global scales
/// until every measured box sits inside the stage safe area. All roles shrink
/// together, so the hero > semi-hero > support hierarchy and the "supports
/// never upscale" rule survive every retry.
pub fn place_with_global_fit(
    ctx: &mut SonnetFlowLayoutContext,
    place: &mut dyn FnMut(&mut [SonnetFlowLayoutBox]),
) {
    let snapshot: Vec<(f64, f64, f64)> = ctx
        .boxes
        .iter()
        .map(|b| (b.font_scale, b.measured_width, b.measured_height))
        .collect();
    let safe_half_w = ctx.width * 0.48;
    let safe_half_h = ctx.height * 0.46;
    for &global_scale in &[1.0f64, 0.92, 0.84, 0.76, 0.68, 0.6, 0.52] {
        for (i, b) in ctx.boxes.iter_mut().enumerate() {
            let (fs, mw, mh) = snapshot[i];
            b.font_scale = fs * global_scale;
            b.measured_width = mw * global_scale;
            b.measured_height = mh * global_scale;
        }
        place(&mut ctx.boxes);
        let fits = ctx.boxes.iter().all(|b| {
            b.x.abs() + b.measured_width / 2.0 <= safe_half_w + 0.5
                && b.y.abs() + b.measured_height / 2.0 <= safe_half_h + 0.5
        });
        if fits {
            return;
        }
    }
}

/// Local helper mirroring TS `xFor` arrow inside `layoutQuietTableau`.
fn x_for_quiet(box_w: f64, index: usize, hero_box_x: f64, hero_box_w: f64, variant: u32) -> f64 {
    if variant == 1 {
        hero_box_x - hero_box_w / 2.0 + box_w / 2.0
    } else if variant == 3 {
        hero_box_x + if index % 2 == 0 { 1.0 } else { -1.0 } * 35.0
    } else {
        hero_box_x
    }
}

// Quiet tableau: one calm stack, earlier words above the hero and later words
// below it, so the column reads top-to-bottom in exact timeline order. When a
// run outgrows the safe height it wraps into side columns instead of
// shrinking: earlier words march right, later words march left — columns
// always read right-to-left in timeline order.
pub fn layout_quiet_tableau(ctx: &mut SonnetFlowLayoutContext, variant: u32) {
    let hero_index = ctx.hero_index;
    let height = ctx.height;
    let stack_gap = ctx.stack_gap;
    let horizontal_card = variant == 2 || variant == 3;
    for b in ctx.boxes.iter_mut() {
        b.layout_direction = if horizontal_card {
            SonnetLayoutDirection::Horizontal
        } else {
            SonnetLayoutDirection::Vertical
        };
    }
    let safe_half_h = height * 0.46;
    place_with_global_fit(ctx, &mut |boxes| {
        {
            let hero_box = &mut boxes[hero_index];
            hero_box.x = 0.0;
            hero_box.y = if horizontal_card { 0.0 } else { -height * 0.1 };
        }
        let hero_box_x = boxes[hero_index].x;
        let hero_box_w = boxes[hero_index].measured_width;
        let hero_box_h = boxes[hero_index].measured_height;
        let hero_box_y = boxes[hero_index].y;
        let stagger = if variant == 3 { 70.0 } else { 0.0 };
        let column_step = boxes
            .iter()
            .map(|b| b.measured_width)
            .fold(f64::NEG_INFINITY, f64::max)
            + stack_gap
            + stagger;
        // Before run: upward from the hero; overflow wraps into columns to the right.
        let mut column: i32 = 0;
        let mut current_y = hero_box_y - hero_box_h / 2.0 - stack_gap;
        for i in (0..hero_index).rev() {
            let b = &mut boxes[i];
            if current_y - b.measured_height < -safe_half_h {
                column += 1;
                current_y = safe_half_h;
            }
            b.x = x_for_quiet(b.measured_width, i, hero_box_x, hero_box_w, variant)
                + column as f64 * column_step;
            b.y = current_y - b.measured_height / 2.0;
            current_y -= b.measured_height + stack_gap;
            if variant == 1 {
                b.enter_x = 20.0;
                b.enter_y = 0.0;
            } else if variant == 3 {
                b.enter_x = if b.x > hero_box_x { 30.0 } else { -30.0 };
                b.enter_y = 0.0;
            } else {
                b.enter_x = 0.0;
                b.enter_y = 20.0;
            }
        }
        // After run: downward from the hero; overflow wraps into columns to the left.
        column = 0;
        current_y = hero_box_y + hero_box_h / 2.0 + stack_gap;
        for i in (hero_index + 1)..boxes.len() {
            let b = &mut boxes[i];
            if current_y + b.measured_height > safe_half_h {
                column += 1;
                current_y = -safe_half_h;
            }
            b.x = x_for_quiet(b.measured_width, i, hero_box_x, hero_box_w, variant)
                - column as f64 * column_step;
            b.y = current_y + b.measured_height / 2.0;
            current_y += b.measured_height + stack_gap;
            if variant == 1 {
                b.enter_x = -20.0;
                b.enter_y = 0.0;
            } else if variant == 3 {
                b.enter_x = if b.x > hero_box_x { 30.0 } else { -30.0 };
                b.enter_y = 0.0;
            } else {
                b.enter_x = 0.0;
                b.enter_y = -20.0;
            }
        }
    });
}

// Tracking ribbon: one horizontal line; words before the hero extend left
// (earliest leftmost), words after extend right — strict reading order.
pub fn layout_tracking_ribbon(ctx: &mut SonnetFlowLayoutContext, variant: u32) {
    let hero_index = ctx.hero_index;
    let flow_gap = ctx.flow_gap;
    for b in ctx.boxes.iter_mut() {
        b.layout_direction = SonnetLayoutDirection::Horizontal;
    }
    place_with_global_fit(ctx, &mut |boxes| {
        {
            let hero_box = &mut boxes[hero_index];
            hero_box.x = 0.0;
            hero_box.y = 0.0;
        }
        let hero_box_y = boxes[hero_index].y;
        let hero_box_h = boxes[hero_index].measured_height;
        let hero_box_w = boxes[hero_index].measured_width;
        let hero_box_x = boxes[hero_index].x;
        let align_y = |box_h: f64, index: usize, variant: u32, hero_y: f64, hero_h: f64| -> f64 {
            if variant == 1 {
                hero_y + hero_h / 2.0 - box_h / 2.0
            } else if variant == 2 {
                hero_y - hero_h / 2.0 + box_h / 2.0
            } else {
                hero_y + if index % 2 == 0 { 10.0 } else { -10.0 }
            }
        };
        let enter = if variant == 2 { 20.0 } else { 30.0 };
        let mut current_x = hero_box_x - hero_box_w / 2.0 - flow_gap;
        for i in (0..hero_index).rev() {
            let b = &mut boxes[i];
            b.x = current_x - b.measured_width / 2.0;
            b.y = align_y(b.measured_height, i, variant, hero_box_y, hero_box_h);
            current_x -= b.measured_width + flow_gap;
            b.enter_x = enter;
            b.enter_y = 0.0;
        }
        current_x = hero_box_x + hero_box_w / 2.0 + flow_gap;
        for i in (hero_index + 1)..boxes.len() {
            let b = &mut boxes[i];
            b.x = current_x + b.measured_width / 2.0;
            b.y = align_y(b.measured_height, i, variant, hero_box_y, hero_box_h);
            current_x += b.measured_width + flow_gap;
            b.enter_x = -enter;
            b.enter_y = 0.0;
        }
    });
}

// Editorial column: five magazine compositions rebuilt around measured flow,
// columns never reverse the timeline and lines never collide.
pub fn layout_editorial_column(
    ctx: &mut SonnetFlowLayoutContext,
    variant: u32,
    secondary_hero_index: usize,
) {
    let hero_index = ctx.hero_index;
    let width = ctx.width;
    let height = ctx.height;
    let flow_gap = ctx.flow_gap;
    let stack_gap = ctx.stack_gap;
    let hero_box_h = ctx.boxes[hero_index].measured_height;
    let hero_box_w = ctx.boxes[hero_index].measured_width;
    if variant == 0 {
        for b in ctx.boxes.iter_mut() {
            b.layout_direction = SonnetLayoutDirection::Vertical;
        }
        place_with_global_fit(ctx, &mut |boxes| {
            let hero_box_x = {
                let hero_box = &mut boxes[hero_index];
                hero_box.x = -width * 0.15;
                hero_box.y = 0.0;
                hero_box.x
            };
            let hero_box_w = boxes[hero_index].measured_width;
            let hero_box_h = boxes[hero_index].measured_height;
            let hero_box_y = boxes[hero_index].y;
            let mut current_y = hero_box_y - hero_box_h / 2.0 + stack_gap * 0.5;
            for i in 0..hero_index {
                let b = &mut boxes[i];
                b.x = hero_box_x + hero_box_w / 2.0 + flow_gap + b.measured_width / 2.0;
                b.y = current_y + b.measured_height / 2.0;
                current_y += b.measured_height + stack_gap;
                b.enter_x = -20.0;
                b.enter_y = 0.0;
            }
            current_y = hero_box_y - hero_box_h / 2.0 + stack_gap * 0.5;
            for i in (hero_index + 1)..boxes.len() {
                let b = &mut boxes[i];
                b.x = hero_box_x - hero_box_w / 2.0 - flow_gap - b.measured_width / 2.0;
                b.y = current_y + b.measured_height / 2.0;
                current_y += b.measured_height + stack_gap;
                b.enter_x = 20.0;
                b.enter_y = 0.0;
            }
        });
    } else if variant == 1 {
        for b in ctx.boxes.iter_mut() {
            b.layout_direction = SonnetLayoutDirection::Vertical;
        }
        place_with_global_fit(ctx, &mut |boxes| {
            let right_edge = width * 0.28;
            let safe_half_h = height * 0.46;
            let rail_step = boxes
                .iter()
                .map(|b| b.measured_width)
                .fold(f64::NEG_INFINITY, f64::max)
                + stack_gap;
            let total_height = boxes.iter().map(|b| b.measured_height).sum::<f64>()
                + stack_gap * (boxes.len() as f64 - 1.0);
            let fits_single_rail =
                boxes.iter().map(|b| b.measured_height).sum::<f64>() * 0.52
                    + stack_gap * (boxes.len() as f64 - 1.0)
                    <= safe_half_h * 2.0;
            if fits_single_rail {
                let mut current_y = -total_height / 2.0;
                for b in boxes.iter_mut() {
                    b.x = right_edge - b.measured_width / 2.0;
                    b.y = current_y + b.measured_height / 2.0;
                    current_y += b.measured_height + stack_gap;
                    b.enter_x = 20.0;
                    b.enter_y = 0.0;
                }
                return;
            }
            let mut rail = 0.0;
            let mut current_y = -safe_half_h;
            for b in boxes.iter_mut() {
                if current_y + b.measured_height > safe_half_h {
                    rail += 1.0;
                    current_y = -safe_half_h;
                }
                b.x = (right_edge - rail * rail_step) - b.measured_width / 2.0;
                b.y = current_y + b.measured_height / 2.0;
                current_y += b.measured_height + stack_gap;
                b.enter_x = 20.0;
                b.enter_y = 0.0;
            }
        });
    } else if variant == 2 {
        for b in ctx.boxes.iter_mut() {
            b.layout_direction = SonnetLayoutDirection::Horizontal;
        }
        place_with_global_fit(ctx, &mut |boxes| {
            let (hero_box_x, hero_box_y, hero_box_w, hero_box_h) = {
                let hero_box = &mut boxes[hero_index];
                hero_box.x = 0.0;
                hero_box.y = -height * 0.25;
                (hero_box.x, hero_box.y, hero_box.measured_width, hero_box.measured_height)
            };
            let before: Vec<usize> = (0..hero_index).collect();
            let after: Vec<usize> = (hero_index + 1..boxes.len()).collect();
            if !before.is_empty() {
                let kicker_height =
                    before.iter().map(|&i| boxes[i].measured_height).fold(f64::NEG_INFINITY, f64::max);
                let kicker_width = before
                    .iter()
                    .map(|&i| boxes[i].measured_width)
                    .sum::<f64>()
                    + flow_gap * (before.len() as f64 - 1.0);
                let kicker_y = hero_box_y - hero_box_h / 2.0 - stack_gap - kicker_height / 2.0;
                let mut current_x = hero_box_x - kicker_width / 2.0;
                for &i in &before {
                    let b = &mut boxes[i];
                    b.x = current_x + b.measured_width / 2.0;
                    b.y = kicker_y;
                    current_x += b.measured_width + flow_gap;
                    b.enter_x = 0.0;
                    b.enter_y = -20.0;
                }
            }
            let left_anchor = hero_box_x - hero_box_w * 0.25 - flow_gap;
            let right_anchor = hero_box_x + hero_box_w * 0.25 + flow_gap;
            let mut current_y = hero_box_y + hero_box_h / 2.0 + stack_gap;
            let mut pair = 0;
            while pair < after.len() {
                let left_i = after[pair];
                let right_i = after.get(pair + 1).copied();
                let left_h = boxes[left_i].measured_height;
                let right_h = right_i.map(|i| boxes[i].measured_height).unwrap_or(0.0);
                let row_height = left_h.max(right_h);
                {
                    let b = &mut boxes[left_i];
                    b.x = left_anchor - b.measured_width / 2.0;
                    b.y = current_y + b.measured_height / 2.0;
                    b.enter_x = -20.0;
                    b.enter_y = 0.0;
                }
                if let Some(ri) = right_i {
                    let b = &mut boxes[ri];
                    b.x = right_anchor + b.measured_width / 2.0;
                    b.y = current_y + b.measured_height / 2.0;
                    b.enter_x = 20.0;
                    b.enter_y = 0.0;
                }
                current_y += row_height + stack_gap;
                pair += 2;
            }
        });
    } else if variant == 3 {
        for b in ctx.boxes.iter_mut() {
            b.layout_direction = SonnetLayoutDirection::Horizontal;
        }
        place_with_global_fit(ctx, &mut |boxes| {
            let (hero_box_x, hero_box_y) = {
                let hero_box = &mut boxes[hero_index];
                hero_box.x = 0.0;
                hero_box.y = 0.0;
                (hero_box.x, hero_box.y)
            };
            let first_hero = (hero_index).min(secondary_hero_index);
            let line1: Vec<usize> = (0..=first_hero).collect();
            let line2: Vec<usize> = (first_hero + 1..boxes.len()).collect();
            let line1_height =
                line1.iter().map(|&i| boxes[i].measured_height).fold(f64::NEG_INFINITY, f64::max);
            let line2_height =
                line2.iter().map(|&i| boxes[i].measured_height).fold(f64::NEG_INFINITY, f64::max);
            let total_height = line1_height + stack_gap + line2_height;
            let line1_y = hero_box_y - total_height / 2.0 + line1_height / 2.0;
            let line2_y = line1_y + line1_height / 2.0 + stack_gap + line2_height / 2.0;
            let lay_line = |boxes: &mut [SonnetFlowLayoutBox], line: &[usize], line_y: f64, enter_x: f64| -> f64 {
                let line_width =
                    line.iter().map(|&i| boxes[i].measured_width).sum::<f64>()
                        + flow_gap * (line.len() as f64 - 1.0);
                let mut current_x = -line_width / 2.0;
                for &i in line {
                    let b = &mut boxes[i];
                    b.x = current_x + b.measured_width / 2.0;
                    b.y = line_y;
                    current_x += b.measured_width + flow_gap;
                    b.enter_x = enter_x;
                    b.enter_y = 0.0;
                }
                line_width
            };
            let line1_w = lay_line(boxes, &line1, line1_y, 30.0);
            let line2_w = lay_line(boxes, &line2, line2_y, -30.0);
            let offset_amount = line1_w.max(line2_w) * 0.12;
            for &i in &line1 {
                boxes[i].x -= offset_amount;
            }
            for &i in &line2 {
                boxes[i].x += offset_amount;
            }
            let _ = hero_box_x;
        });
    } else if variant == 4 {
        for (idx, b) in ctx.boxes.iter_mut().enumerate() {
            b.layout_direction = if idx == hero_index {
                SonnetLayoutDirection::Vertical
            } else {
                SonnetLayoutDirection::Horizontal
            };
        }
        place_with_global_fit(ctx, &mut |boxes| {
            let hero_on_right = hero_index == boxes.len() - 1;
            let block_left = -width * 0.40;
            let block_right = width * 0.40;
            let mut current_y = -height * 0.34;
            let hero_box_w = boxes[hero_index].measured_width;
            let hero_box_h = boxes[hero_index].measured_height;
            let before_indices: Vec<usize> = (0..hero_index).collect();
            let after_indices: Vec<usize> = (hero_index + 1..boxes.len()).collect();
            let mut flow_words = |boxes: &mut [SonnetFlowLayoutBox], current_y: &mut f64, indices: &[usize], region_for: &dyn Fn(f64) -> (f64, f64)| {
                let (mut left, mut right) = region_for(*current_y);
                let mut current_x = left;
                let mut row_height = 0.0f64;
                for &index in indices {
                    let b = &mut boxes[index];
                    if current_x > left && current_x + b.measured_width > right {
                        *current_y += row_height + stack_gap;
                        let (l, r) = region_for(*current_y);
                        left = l;
                        right = r;
                        current_x = left;
                        row_height = 0.0;
                    }
                    b.x = current_x + b.measured_width / 2.0;
                    b.y = *current_y + b.measured_height / 2.0;
                    b.enter_x = if hero_on_right { -25.0 } else { 25.0 };
                    b.enter_y = 0.0;
                    current_x += b.measured_width + flow_gap;
                    if b.measured_height > row_height {
                        row_height = b.measured_height;
                    }
                }
                if !indices.is_empty() {
                    *current_y += row_height;
                }
            };
            flow_words(boxes, &mut current_y, &before_indices, &|_row_top| (block_left, block_right));
            current_y += stack_gap;
            {
                let hero_box = &mut boxes[hero_index];
                let pillar_left = if hero_on_right { block_right - hero_box_w } else { block_left };
                hero_box.x = pillar_left + hero_box_w / 2.0;
                hero_box.y = current_y + hero_box_h / 2.0;
            }
            let pillar_bottom = current_y + hero_box_h + stack_gap;
            let pillar_left_for_region = if hero_on_right { block_right - hero_box_w } else { block_left };
            let beside_left = if hero_on_right { block_left } else { pillar_left_for_region + hero_box_w + flow_gap };
            let beside_right = if hero_on_right { pillar_left_for_region - flow_gap } else { block_right };
            flow_words(
                boxes,
                &mut current_y,
                &after_indices,
                &|row_top| {
                    if row_top < pillar_bottom - 0.5 {
                        (beside_left, beside_right)
                    } else {
                        (block_left, block_right)
                    }
                },
            );
        });
    }
    let _ = (hero_box_h, hero_box_w);
}

// Fragment collage: polar orbit in strict clockwise timeline order. Each support
// advances its angle until its measured rect clears every rect already placed
// (hero included), so the ring keeps its chaotic look without overlap.
pub fn layout_fragment_collage(ctx: &mut SonnetFlowLayoutContext, variant: u32) {
    let hero_index = ctx.hero_index;
    let flow_gap = ctx.flow_gap;
    let stack_gap = ctx.stack_gap;
    // Flatten rotated non-CJK blocks back to horizontal before global-fit retries.
    for (idx, b) in ctx.boxes.iter_mut().enumerate() {
        if idx == hero_index {
            continue;
        }
        if (b.rotation / (std::f64::consts::PI / 2.0)).round().abs() as i64 % 2 == 1 {
            let rotated_width = b.measured_height;
            b.measured_height = b.measured_width;
            b.measured_width = rotated_width;
        }
        b.rotation = 0.0;
    }
    #[derive(Clone, Copy)]
    struct PlacedRect {
        left: f64,
        right: f64,
        top: f64,
        bottom: f64,
    }
    let rect_separation = |a: PlacedRect, b: PlacedRect| -> f64 {
        (a.left - b.right).max(b.left - a.right).max(a.top - b.bottom).max(b.top - a.bottom)
    };
    place_with_global_fit_with_scale(ctx, &mut |boxes, global_scale| {
        let hero_w = boxes[hero_index].measured_width;
        let hero_h = boxes[hero_index].measured_height;
        {
            let hero_box = &mut boxes[hero_index];
            hero_box.x = 0.0;
            hero_box.y = 0.0;
        }
        let base_radius = (hero_w.hypot(hero_h)) / 2.0 + stack_gap;
        let count = (boxes.len() - 1).max(1) as f64;
        let squash = 0.65;
        let mut placed: Vec<PlacedRect> = vec![PlacedRect {
            left: -hero_w / 2.0,
            right: hero_w / 2.0,
            top: -hero_h / 2.0,
            bottom: hero_h / 2.0,
        }];
        let mut angle = std::f64::consts::PI / 4.0;
        let mut support_index = 0usize;
        for i in 0..boxes.len() {
            if i == hero_index {
                continue;
            }
            let mut radius = base_radius;
            if variant == 1 {
                radius += (35.0 + (support_index as f64 / count) * 150.0) * global_scale;
            } else if variant == 2 {
                radius += (if support_index % 2 == 1 { 140.0 } else { 50.0 }) * global_scale;
            } else {
                radius += (45.0 + ((support_index * 23) as f64 % 90.0)) * global_scale;
            }
            support_index += 1;
            let mut candidate = angle;
            let mut rect = PlacedRect { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };
            let mut resolved_radius = radius;
            let mut placed_clear = false;
            for ring in 0..14 {
                if placed_clear {
                    break;
                }
                for _attempt in 0..400 {
                    rect = PlacedRect {
                        left: candidate.cos() * resolved_radius
                            - boxes[i].measured_width / 2.0,
                        right: candidate.cos() * resolved_radius
                            + boxes[i].measured_width / 2.0,
                        top: candidate.sin() * resolved_radius * squash
                            - boxes[i].measured_height / 2.0,
                        bottom: candidate.sin() * resolved_radius * squash
                            + boxes[i].measured_height / 2.0,
                    };
                    if placed
                        .iter()
                        .all(|entry| rect_separation(*entry, rect) >= flow_gap)
                    {
                        placed_clear = true;
                        break;
                    }
                    candidate += 0.07;
                }
                if !placed_clear {
                    resolved_radius += (36.0 + ring as f64 * 12.0) * global_scale;
                }
            }
            angle = candidate + 0.02;
            placed.push(rect);
            {
                let b = &mut boxes[i];
                b.x = candidate.cos() * resolved_radius;
                b.y = candidate.sin() * resolved_radius * squash;
                b.layout_direction =
                    if candidate.cos().abs() >= candidate.sin().abs() {
                        SonnetLayoutDirection::Vertical
                    } else {
                        SonnetLayoutDirection::Horizontal
                    };
                b.enter_x = candidate.cos() * -60.0;
                b.enter_y = candidate.sin() * -60.0;
            }
        }
    });
}

// Dynamic cross (type-impact / mask-reveal): top column -> left row -> hero ->
// right row -> bottom column. Band split keeps scan order equal to timeline order.
pub fn layout_cross_stack(ctx: &mut SonnetFlowLayoutContext) {
    let hero_index = ctx.hero_index;
    let height = ctx.height;
    let flow_gap = ctx.flow_gap;
    let stack_gap = ctx.stack_gap;
    let before_count = hero_index;
    let top_count = before_count / 2;
    let after_count = ctx.boxes.len() - 1 - hero_index;
    let right_count = (after_count + 1) / 2;

    let fill_column = |boxes: &mut [SonnetFlowLayoutBox], column: &[usize]| -> f64 {
        if column.is_empty() {
            return 0.0;
        }
        let hero_h = boxes[hero_index].measured_height;
        let available = (height * 0.46 - hero_h / 2.0 - stack_gap).max(0.0);
        if available <= 0.0 {
            return 0.0;
        }
        let gaps = stack_gap * (column.len() as f64 - 1.0);
        let content_height: f64 = column.iter().map(|&i| boxes[i].measured_height).sum();
        let target = available * 0.72;
        if content_height + gaps < target {
            let boost = ((target - gaps) / content_height.max(1.0)).min(2.2);
            for &i in column {
                let hero_fs = boxes[hero_index].font_scale;
                let capped = boost.min((hero_fs * 0.6) / boxes[i].font_scale);
                if capped > 1.05 {
                    let b = &mut boxes[i];
                    b.font_scale *= capped;
                    b.measured_width *= capped;
                    b.measured_height *= capped;
                }
            }
        }
        if column.len() < 2 {
            return 0.0;
        }
        let grown: f64 = column.iter().map(|&i| boxes[i].measured_height).sum();
        let pitch = (available * 0.95 - grown) / (column.len() as f64 - 1.0);
        (0.0f64).max((stack_gap * 2.0).min(pitch - stack_gap))
    };

    place_with_global_fit(ctx, &mut |boxes| {
        let (hero_w, hero_h) = {
            let hero_box = &mut boxes[hero_index];
            hero_box.x = 0.0;
            hero_box.y = 0.0;
            (hero_box.measured_width, hero_box.measured_height)
        };
        let top_indices: Vec<usize> = (0..top_count).collect();
        let bottom_indices: Vec<usize> = ((hero_index + right_count + 1)..boxes.len()).collect();
        // top/bottom stretch must be computed AFTER hero layout (on shrunk hero).
        let top_stretch = fill_column(boxes, &top_indices);
        let bottom_stretch = fill_column(boxes, &bottom_indices);
        // Left row: indices topCount..heroIndex-1, earliest ends up leftmost (walk right-to-left from hero-1).
        let mut current_x = -hero_w / 2.0 - stack_gap;
        for i in (top_count..hero_index).rev() {
            let b = &mut boxes[i];
            b.layout_direction = SonnetLayoutDirection::Horizontal;
            b.x = current_x - b.measured_width / 2.0;
            b.y = if i % 2 == 0 { 10.0 } else { -10.0 };
            current_x -= b.measured_width + flow_gap;
            b.enter_x = -30.0;
            b.enter_y = 0.0;
        }
        // Top column: indices 0..topCount-1, earliest ends up topmost (walk top-to-bottom but from topCount-1 up).
        let mut current_y = -hero_h / 2.0 - stack_gap;
        for i in (0..top_count).rev() {
            let b = &mut boxes[i];
            b.layout_direction = SonnetLayoutDirection::Vertical;
            b.x = if i % 2 == 0 { 15.0 } else { -15.0 };
            b.y = current_y - b.measured_height / 2.0;
            current_y -= b.measured_height + stack_gap + top_stretch;
            b.enter_x = 0.0;
            b.enter_y = -30.0;
        }
        // Right row: the words right after the hero, left-to-right.
        current_x = hero_w / 2.0 + stack_gap;
        for i in (hero_index + 1)..=(hero_index + right_count) {
            let b = &mut boxes[i];
            b.layout_direction = SonnetLayoutDirection::Horizontal;
            b.x = current_x + b.measured_width / 2.0;
            b.y = if i % 2 == 0 { 10.0 } else { -10.0 };
            current_x += b.measured_width + flow_gap;
            b.enter_x = 30.0;
            b.enter_y = 0.0;
        }
        // Bottom column: remaining words, top-to-bottom.
        current_y = hero_h / 2.0 + stack_gap;
        for i in (hero_index + right_count + 1)..boxes.len() {
            let b = &mut boxes[i];
            b.layout_direction = SonnetLayoutDirection::Vertical;
            b.x = if i % 2 == 0 { 15.0 } else { -15.0 };
            b.y = current_y + b.measured_height / 2.0;
            current_y += b.measured_height + stack_gap + bottom_stretch;
            b.enter_x = 0.0;
            b.enter_y = 30.0;
        }
    });
}

/// `placeWithGlobalFit` variant that exposes the current global scale, used by
/// fragment collage which needs it for the radial scaffold.
fn place_with_global_fit_with_scale(
    ctx: &mut SonnetFlowLayoutContext,
    place: &mut dyn FnMut(&mut [SonnetFlowLayoutBox], f64),
) {
    let snapshot: Vec<(f64, f64, f64)> =
        ctx.boxes.iter().map(|b| (b.font_scale, b.measured_width, b.measured_height)).collect();
    let safe_half_w = ctx.width * 0.48;
    let safe_half_h = ctx.height * 0.46;
    for &global_scale in &[1.0f64, 0.92, 0.84, 0.76, 0.68, 0.6, 0.52] {
        for (i, b) in ctx.boxes.iter_mut().enumerate() {
            let (fs, mw, mh) = snapshot[i];
            b.font_scale = fs * global_scale;
            b.measured_width = mw * global_scale;
            b.measured_height = mh * global_scale;
        }
        place(&mut ctx.boxes, global_scale);
        let fits = ctx.boxes.iter().all(|b| {
            b.x.abs() + b.measured_width / 2.0 <= safe_half_w + 0.5
                && b.y.abs() + b.measured_height / 2.0 <= safe_half_h + 0.5
        });
        if fits {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_box(index: usize, is_hero: bool) -> SonnetFlowLayoutBox {
        SonnetFlowLayoutBox {
            index,
            is_hero,
            is_semi_hero: false,
            display_text: format!("w{}", index),
            font_scale: 1.0,
            measured_width: 100.0,
            measured_height: 40.0,
            vertical: false,
            layout_direction: SonnetLayoutDirection::Horizontal,
            rotation: 0.0,
            x: 0.0,
            y: 0.0,
            enter_x: 0.0,
            enter_y: 0.0,
        }
    }

    fn ctx(
        boxes: Vec<SonnetFlowLayoutBox>,
        hero_index: usize,
        width: f64,
        height: f64,
        flow_gap: f64,
        stack_gap: f64,
    ) -> SonnetFlowLayoutContext {
        SonnetFlowLayoutContext {
            boxes,
            hero_index,
            width,
            height,
            flow_gap,
            stack_gap,
        }
    }

    #[test]
    fn resolve_sonnet_flow_gaps_clamps_to_range() {
        let (g, s) = resolve_sonnet_flow_gaps(100.0);
        assert_eq!(g, 35.0); // 100 * 0.35
        assert_eq!(s, 35.0 * 1.35);
        let (g, s) = resolve_sonnet_flow_gaps(10.0);
        assert_eq!(g, 16.0); // min
        assert_eq!(s, 24.0); // >= 24
        let (g, _) = resolve_sonnet_flow_gaps(200.0);
        assert_eq!(g, 40.0); // max
    }

    #[test]
    fn tracking_ribbon_earliest_leftmost_later_right() {
        // 3 boxes, hero_index=1: [before, hero, after]
        let boxes = vec![mk_box(0, false), mk_box(1, true), mk_box(2, false)];
        let mut c = ctx(boxes, 1, 1000.0, 100.0, 30.0, 35.0);
        layout_tracking_ribbon(&mut c, 0);
        assert!(c.boxes[0].x < c.boxes[1].x);
        assert!(c.boxes[2].x > c.boxes[1].x);
        assert_eq!(c.boxes[1].x, 0.0);
        // before on the left extends leftward
        assert_eq!(c.boxes[0].enter_x, 30.0);
        // after extends right with negative enter
        assert_eq!(c.boxes[2].enter_x, -30.0);
    }

    #[test]
    fn quiet_tableau_before_above_after_below() {
        // Use height=400 so the before/after boxes fit above/below the hero
        // without wrapping into side columns (wrapping changes vertical order).
        let boxes = vec![mk_box(0, false), mk_box(1, true), mk_box(2, false)];
        let mut c = ctx(boxes, 1, 1000.0, 400.0, 30.0, 35.0);
        layout_quiet_tableau(&mut c, 0);
        // vertical layout
        assert_eq!(c.boxes[1].layout_direction, SonnetLayoutDirection::Vertical);
        // hero at y = -height*0.1 = -40; before above, after below (variant 0)
        assert!(c.boxes[0].y < c.boxes[1].y, "before {} should be above hero {}", c.boxes[0].y, c.boxes[1].y);
        assert!(c.boxes[2].y > c.boxes[1].y, "after {} should be below hero {}", c.boxes[2].y, c.boxes[1].y);
    }
}
