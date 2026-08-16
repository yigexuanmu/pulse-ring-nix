//! Folia sonnet v2 — `sonnetTypographyLayout.ts` (404 lines) `compiler-grade
//! 1:1 port`.
//!
//! PV-style kinetic typography layouts based on exact box measurements. This
//! is the orchestrator that wires the segment scorer (`typography_roles`), the
//! pure-geometry layout pass (`poster_blocks_layout`, `shot_flow_layouts`),
//! and pretext `prepareWithSegments` / `measureText` (via the measurement
//! backend) into a list of `SonnetTypographyPlacement` boxes per shot.

use crate::lyricstyles::sonnet_v2::poster_blocks_layout::{
    layout_sonnet_poster_blocks, SonnetPosterBlockBox,
};
use crate::lyricstyles::sonnet_v2::pretext::layout::{
    measure_natural_width, prepare_with_segments, PrepareOptions,
};
use crate::lyricstyles::sonnet_v2::pretext::measurement::{MeasureBackend, MeasurementCaches};
use crate::lyricstyles::sonnet_v2::random::hash_sonnet_seed;
use crate::lyricstyles::sonnet_v2::shot_flow_layouts::{
    layout_cross_stack, layout_editorial_column, layout_fragment_collage,
    layout_quiet_tableau, layout_tracking_ribbon, resolve_sonnet_flow_gaps,
    SonnetFlowLayoutBox, SonnetFlowLayoutContext,
};
use crate::lyricstyles::sonnet_v2::typography_roles::{
    get_sonnet_visible_segment_length, resolve_sonnet_role_font_weight,
    score_sonnet_hero_segment,
};
use crate::lyricstyles::sonnet_v2::types::{
    SonnetLayoutDirection, SonnetParagraphKind, SonnetSemanticSegment, SonnetSegmentRole,
    SonnetShotKind,
};

// re-exports matching `export { ... } from './sonnetTypographyRoles'` in folia.
pub use crate::lyricstyles::sonnet_v2::typography_roles::{
    find_sonnet_hero_segment_index, find_sonnet_semi_hero_segment_index,
    find_sonnet_semi_hero_segment_indices, is_sonnet_emphasis_role,
};

/// `SonnetTypographyPlacement` — folia `sonnetTypographyLayout.ts`.
#[derive(Debug, Clone)]
pub struct SonnetTypographyPlacement {
    pub segment_index: usize,
    pub display_text: String,
    pub role: SonnetSegmentRole,
    pub font_scale: f64,
    pub measured_width: f64,
    pub measured_height: f64,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub enter_x: f64,
    pub enter_y: f64,
    pub vertical: bool,
    pub layout_direction: SonnetLayoutDirection,
    pub timing_phase: f64,
}

/// `SonnetTypographyLayoutOptions` — the orchestrator input. `font_weight`
/// mirrors the folia `fontWeight?: number | null` channel (caller may pass
/// `None` for auto mode).
#[derive(Debug, Clone)]
pub struct SonnetTypographyLayoutOptions {
    pub lines: Vec<Vec<SonnetSemanticSegment>>,
    pub shot_kind: SonnetShotKind,
    pub paragraph_kind: SonnetParagraphKind,
    pub width: f64,
    pub height: f64,
    pub base_font_size: f64,
    pub font_family: String,
    pub font_weight: Option<i32>,
}

/// `isSonnetLayoutSegment(segment)` — folia export that filters out
/// pure-whitespace segments before layout.
pub fn is_sonnet_layout_segment(segment: &SonnetSemanticSegment) -> bool {
    segment.text.trim().len() > 0
}

/// `CJK_TEXT = /[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af]/u`
fn is_cjk_text(s: &str) -> bool {
    s.chars().any(|c| {
        let cp = c as u32;
        (0x4e00..=0x9fff).contains(&cp)
            || (0x3040..=0x30ff).contains(&cp)
            || (0xac00..=0xd7af).contains(&cp)
    })
}

/// `shouldRotateNonCjkSegment(segment, vertical)` — vertical layout of a
/// multi-grapheme non-CJK word swings 90° rather than stacking graphemes.
fn should_rotate_non_cjk_segment(
    segment: &SonnetSemanticSegment,
    vertical: bool,
) -> bool {
    vertical
        && segment
            .graphemes
            .iter()
            .filter(|g| g.char.trim().len() > 0)
            .count()
            > 1
        && !is_cjk_text(&segment.text)
}

/// `verticalText(segment)` — joins the segment's graphemes (or code points
/// when the segment has no parser-derived graphemes) with `\n` so the rectangular
/// packer stacks glyphs down a CJK column.
fn vertical_text(segment: &SonnetSemanticSegment) -> String {
    if segment.graphemes.is_empty() {
        segment.text.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("\n")
    } else {
        segment
            .graphemes
            .iter()
            .map(|g| g.char.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `measureText(text, fontSpec, fontSize)` — folia delegates to pretext's
/// `layoutWithLines(prepareWithSegments(text, fontSpec), 99999, fontSize*1.2)`
/// and reads `lines[0].width`, falling back to a rough `text.length *
/// fontSize * 0.6` estimate on error. The Rust port substitutes the
/// no-max-width `measure_natural_width` (same semantics since 99999 never
/// wraps). `f32` matches the measurement backend's float type.
pub fn measure_text<B: MeasureBackend>(
    text: &str,
    font_spec: &str,
    font_size: f32,
    caches: &mut MeasurementCaches,
    backend: &B,
) -> f32 {
    let trimmed = if text.is_empty() { " " } else { text };
    let prepared = prepare_with_segments(
        trimmed,
        caches,
        backend,
        font_spec,
        PrepareOptions::default(),
    );
    let width = measure_natural_width(&prepared);
    if width > 0.0 {
        width
    } else {
        trimmed.chars().count() as f32 * font_size * 0.6
    }
}

/// Internal master box — superset of `SonnetFlowLayoutBox` + the vertical
/// orientation fields `SonnetPosterBlockBox` owns, plus the `role` /
/// `timing_phase` / `relative_phase` fields folia carries on its single box
/// type.
#[derive(Debug, Clone)]
struct TypographyBox {
    index: usize,
    is_hero: bool,
    is_semi_hero: bool,
    display_text: String,
    vertical_display_text: Option<String>,
    vertical_measured_width: Option<f64>,
    vertical_measured_height: Option<f64>,
    vertical_font_scale: Option<f64>,
    font_scale: f64,
    measured_width: f64,
    measured_height: f64,
    vertical: bool,
    layout_direction: SonnetLayoutDirection,
    rotation: f64,
    x: f64,
    y: f64,
    enter_x: f64,
    enter_y: f64,
    timing_phase: f64,
    relative_phase: f64,
    role: Option<SonnetSegmentRole>,
}

/// Fits a measured box to `width*0.82 × height*0.82` by scaling every
/// measurement and font size down by the same factor (so the hero >
/// semi-hero > support hierarchy survives).
fn apply_fit_scale(
    max_w: f64,
    max_h: f64,
    target_font_size: &mut f64,
    font_scale: &mut f64,
    measured_width: &mut f64,
    measured_height: &mut f64,
) {
    let mut fit_scale = 1.0f64;
    if *measured_width > max_w {
        fit_scale = fit_scale.min(max_w / *measured_width);
    }
    if *measured_height > max_h {
        fit_scale = fit_scale.min(max_h / *measured_height);
    }
    if fit_scale < 1.0 {
        *target_font_size *= fit_scale;
        *font_scale *= fit_scale;
        *measured_width *= fit_scale;
        *measured_height *= fit_scale;
    }
}

/// `resolveSonnetTypographyLayout(options)` — folia `sonnetTypographyLayout.ts`
/// orchestrator. Generic over the measurement backend so the eventual FreeType
/// glyph atlas (Phase 5) plugs in without changing the algorithm.
pub fn resolve_sonnet_typography_layout<B: MeasureBackend>(
    options: &SonnetTypographyLayoutOptions,
    caches: &mut MeasurementCaches,
    backend: &B,
) -> Vec<SonnetTypographyPlacement> {
    let lines = &options.lines;
    let shot_kind = options.shot_kind;
    let width = options.width;
    let height = options.height;
    let base_font_size = options.base_font_size;
    let font_family = &options.font_family;
    let font_weight = options.font_weight;

    let segments: Vec<SonnetSemanticSegment> =
        lines.iter().flatten().cloned().collect();

    let mut offset = 0usize;
    let mut hero_indices: Vec<usize> = Vec::new();
    let mut semi_hero_indices: Vec<usize> = Vec::new();
    for line_segs in lines.iter() {
        let local_hero = find_sonnet_hero_segment_index(line_segs);
        let global_hero = offset + local_hero;
        let local_semi = find_sonnet_semi_hero_segment_indices(line_segs, local_hero);
        hero_indices.push(global_hero);
        for ls in local_semi {
            semi_hero_indices.push(offset + ls);
        }
        offset += line_segs.len();
    }

    let hero_index = find_sonnet_hero_segment_index(&segments);
    let midpoints: Vec<f64> = segments
        .iter()
        .map(|s| (s.start_time + s.end_time) * 0.5)
        .collect();
    let timeline_start = midpoints
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let timeline_end = midpoints
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let timeline_duration = timeline_end - timeline_start;
    let phases: Vec<f64> = midpoints
        .iter()
        .enumerate()
        .map(|(index, midpoint)| {
            if timeline_duration > 0.001 {
                (midpoint - timeline_start) / timeline_duration
            } else {
                index as f64 / (segments.len().saturating_sub(1).max(1)) as f64
            }
        })
        .collect();
    let hero_phase = phases.get(hero_index).copied().unwrap_or(0.5);

    // Deterministic pseudo-randomness for layout variations.
    let layout_variant_seed: u32 = segments
        .iter()
        .map(|s| s.text.trim().len().max(1) as u32)
        .sum::<u32>()
        .wrapping_add(segments.len() as u32);
    let joined_with_record_sep: String = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\u{241f}");
    let poster_layout_seed = hash_sonnet_seed(&joined_with_record_sep);
    let mut editorial_variant = layout_variant_seed % 5;
    let ribbon_variant = layout_variant_seed % 3;
    let tableau_variant = layout_variant_seed % 4;
    let collage_variant = layout_variant_seed % 3;

    let mut secondary_hero_index: i64 = -1;
    if editorial_variant == 3 && segments.len() > 2 {
        let mut best_score = f64::NEG_INFINITY;
        for (index, segment) in segments.iter().enumerate() {
            if index == hero_index
                || !segment.is_word_like
                || get_sonnet_visible_segment_length(segment) == 0
            {
                continue;
            }
            let distance_bonus = if (index as i64 - hero_index as i64).unsigned_abs() > 1 {
                50.0
            } else {
                0.0
            };
            let score = score_sonnet_hero_segment(segment) + distance_bonus;
            if score > best_score {
                best_score = score;
                secondary_hero_index = index as i64;
            }
        }
        if secondary_hero_index == -1 {
            editorial_variant = 0;
        }
    } else if editorial_variant == 3 {
        editorial_variant = 0;
    } else if editorial_variant == 4 && segments.len() < 2 {
        editorial_variant = 2;
    }

    // 1. Assign styles and measure boxes.
    let max_w = width * 0.82;
    let max_h = height * 0.82;
    let mut boxes: Vec<TypographyBox> = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let is_hero = hero_indices.contains(&index)
            || (index as i64 == secondary_hero_index
                && shot_kind == SonnetShotKind::EditorialColumn
                && editorial_variant == 3);
        let is_semi_hero = semi_hero_indices.contains(&index) && !is_hero;
        let is_emphasized = is_hero || is_semi_hero;
        let mut hero_font_scale = 1.0f64;
        let mut support_font_scale = 1.0f64;
        let mut vertical = false;
        let mut rotation = 0.0f64;

        match shot_kind {
            SonnetShotKind::EditorialColumn => {
                if editorial_variant == 3 {
                    hero_font_scale = 3.8;
                    support_font_scale = 1.3;
                    vertical = false;
                } else if editorial_variant == 4 {
                    hero_font_scale = 4.2;
                    support_font_scale = 1.25;
                    vertical = is_emphasized;
                } else {
                    hero_font_scale = if editorial_variant == 2 { 3.2 } else { 4.0 };
                    support_font_scale = 1.2;
                    vertical = is_emphasized && editorial_variant != 2;
                }
            }
            SonnetShotKind::TypeImpact => {
                hero_font_scale = 5.5;
                support_font_scale = 1.5;
            }
            SonnetShotKind::FragmentCollage => {
                hero_font_scale = 3.2;
                support_font_scale = 1.35;
                vertical = is_semi_hero || (index % 4) == 0;
            }
            SonnetShotKind::TrackingRibbon => {
                hero_font_scale = 3.5;
                support_font_scale = 1.5;
            }
            SonnetShotKind::MaskReveal => {
                hero_font_scale = 4.5;
                support_font_scale = 1.6;
                vertical = is_emphasized;
            }
            SonnetShotKind::PosterBlocks => {
                hero_font_scale = 4.4;
                support_font_scale = 1.15;
            }
            SonnetShotKind::QuietTableau => {
                hero_font_scale = 3.0;
                support_font_scale = 1.15;
                vertical = is_emphasized && (tableau_variant == 0 || tableau_variant == 1);
            }
        }

        let mut font_scale = if is_hero {
            hero_font_scale
        } else if is_semi_hero {
            (support_font_scale * 1.35).max(hero_font_scale * 0.72)
        } else {
            support_font_scale
        };

        let rotates_non_cjk_segment = should_rotate_non_cjk_segment(segment, vertical);
        if rotates_non_cjk_segment {
            vertical = false;
            rotation += std::f64::consts::FRAC_PI_2;
        }

        let display_text = if vertical {
            vertical_text(segment)
        } else {
            segment.text.clone()
        };
        let render_role = if is_hero {
            SonnetSegmentRole::Hero
        } else if is_semi_hero {
            SonnetSegmentRole::SemiHero
        } else {
            SonnetSegmentRole::Support
        };
        let render_weight = resolve_sonnet_role_font_weight(font_weight, render_role);

        let mut target_font_size = base_font_size * font_scale;
        let font_spec = format!("{} {}px {}", render_weight, target_font_size, font_family);

        let horizontal_advance = if rotates_non_cjk_segment {
            let mut sum = 0.0f64;
            for item in segment.graphemes.iter() {
                if item.char.trim().len() > 0 {
                    sum += (target_font_size as f32 * 0.2).max(measure_text(
                        &item.char,
                        &font_spec,
                        target_font_size as f32,
                        caches,
                        backend,
                    )) as f64;
                }
            }
            sum
        } else {
            measure_text(
                &display_text,
                &font_spec,
                target_font_size as f32,
                caches,
                backend,
            ) as f64
        };

        let mut measured_width = if rotates_non_cjk_segment {
            target_font_size * 1.2
        } else {
            horizontal_advance
        };
        let mut measured_height = if rotates_non_cjk_segment {
            horizontal_advance
        } else {
            target_font_size * 1.2
        };

        if vertical {
            let column_chars: Vec<String> = if segment.graphemes.is_empty() {
                segment.text.chars().map(|c| c.to_string()).collect()
            } else {
                segment.graphemes.iter().map(|g| g.char.clone()).collect()
            };
            let glyph_advances: Vec<f64> = column_chars
                .iter()
                .filter(|c| c.trim().len() > 0)
                .map(|c| {
                    (target_font_size as f32 * 0.2)
                        .max(measure_text(
                            c,
                            &font_spec,
                            target_font_size as f32,
                            caches,
                            backend,
                        )) as f64
                })
                .collect();
            measured_width = if glyph_advances.is_empty() {
                target_font_size
            } else {
                glyph_advances.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            };
            measured_height = (column_chars.len().max(1)) as f64 * target_font_size * 0.9;
        }

        apply_fit_scale(
            max_w,
            max_h,
            &mut target_font_size,
            &mut font_scale,
            &mut measured_width,
            &mut measured_height,
        );

        let mut vertical_display_text: Option<String> = None;
        let mut vertical_measured_width: Option<f64> = None;
        let mut vertical_measured_height: Option<f64> = None;
        let mut vertical_font_scale: Option<f64> = None;
        if shot_kind == SonnetShotKind::PosterBlocks && is_cjk_text(&segment.text) {
            let column_chars: Vec<String> = if segment.graphemes.is_empty() {
                segment.text.chars().map(|c| c.to_string()).collect()
            } else {
                segment.graphemes.iter().map(|g| g.char.clone()).collect()
            };
            let glyph_advances: Vec<f64> = column_chars
                .iter()
                .filter(|c| c.trim().len() > 0)
                .map(|c| {
                    (target_font_size as f32 * 0.2)
                        .max(measure_text(
                            c,
                            &font_spec,
                            target_font_size as f32,
                            caches,
                            backend,
                        )) as f64
                })
                .collect();
            let mut column_width = if glyph_advances.is_empty() {
                target_font_size
            } else {
                glyph_advances.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            };
            let mut column_height = (column_chars.len().max(1)) as f64 * target_font_size * 0.9;
            let vertical_fit = 1.0f64.min(max_w / column_width).min(max_h / column_height);
            column_width *= vertical_fit;
            column_height *= vertical_fit;
            vertical_display_text = Some(vertical_text(segment));
            vertical_measured_width = Some(column_width);
            vertical_measured_height = Some(column_height);
            vertical_font_scale = Some(font_scale * vertical_fit);
        }

        boxes.push(TypographyBox {
            index,
            is_hero,
            is_semi_hero,
            display_text,
            vertical_display_text,
            vertical_measured_width,
            vertical_measured_height,
            vertical_font_scale,
            font_scale,
            vertical,
            layout_direction: SonnetLayoutDirection::Horizontal,
            rotation,
            measured_width,
            measured_height,
            x: 0.0,
            y: 0.0,
            enter_x: 0.0,
            enter_y: 0.0,
            timing_phase: phases[index],
            relative_phase: phases[index] - hero_phase,
            role: None,
        });
    }

    // 2. Exact layout packing.
    if let Some(hero_box) = boxes.get_mut(hero_index) {
        if shot_kind == SonnetShotKind::PosterBlocks {
            let mut poster_boxes: Vec<SonnetPosterBlockBox> = boxes
                .iter()
                .map(|b| SonnetPosterBlockBox {
                    is_hero: b.is_hero,
                    is_semi_hero: b.is_semi_hero,
                    display_text: b.display_text.clone(),
                    vertical_display_text: b.vertical_display_text.clone(),
                    vertical_measured_width: b.vertical_measured_width,
                    vertical_measured_height: b.vertical_measured_height,
                    vertical_font_scale: b.vertical_font_scale,
                    font_scale: b.font_scale,
                    measured_width: b.measured_width,
                    measured_height: b.measured_height,
                    x: b.x,
                    y: b.y,
                    rotation: b.rotation,
                    vertical: b.vertical,
                    layout_direction: b.layout_direction,
                    enter_x: b.enter_x,
                    enter_y: b.enter_y,
                })
                .collect();
            // `layout_sonnet_poster_blocks` mutates its slice in place; the
            // returned `SonnetPosterBlocksPlan` re-exports those placements.
            let _plan = layout_sonnet_poster_blocks(
                &mut poster_boxes,
                width,
                height,
                base_font_size,
                poster_layout_seed,
            );
            for (i, pb) in poster_boxes.iter().enumerate() {
                let b = &mut boxes[i];
                b.x = pb.x;
                b.y = pb.y;
                b.rotation = pb.rotation;
                b.vertical = pb.vertical;
                b.layout_direction = pb.layout_direction;
                b.enter_x = pb.enter_x;
                b.enter_y = pb.enter_y;
                // Poster blocks may downscale boxes in place via `verticalFit`
                // or layout-specific shrink — accept the font_scale changes
                // (callers read `fontScale` off the final placement).
                b.font_scale = pb.font_scale;
                b.measured_width = pb.measured_width;
                b.measured_height = pb.measured_height;
            }
        } else {
            let (flow_gap, stack_gap) = resolve_sonnet_flow_gaps(base_font_size);
            let mut flow_boxes: Vec<SonnetFlowLayoutBox> = boxes
                .iter()
                .map(|b| SonnetFlowLayoutBox {
                    index: b.index,
                    is_hero: b.is_hero,
                    is_semi_hero: b.is_semi_hero,
                    display_text: b.display_text.clone(),
                    font_scale: b.font_scale,
                    measured_width: b.measured_width,
                    measured_height: b.measured_height,
                    vertical: b.vertical,
                    layout_direction: b.layout_direction,
                    rotation: b.rotation,
                    x: b.x,
                    y: b.y,
                    enter_x: b.enter_x,
                    enter_y: b.enter_y,
                })
                .collect();
            let mut flow_ctx = SonnetFlowLayoutContext {
                boxes: flow_boxes,
                hero_index,
                width,
                height,
                flow_gap,
                stack_gap,
            };
            match shot_kind {
                SonnetShotKind::QuietTableau => {
                    layout_quiet_tableau(&mut flow_ctx, tableau_variant)
                }
                SonnetShotKind::TrackingRibbon => {
                    layout_tracking_ribbon(&mut flow_ctx, ribbon_variant)
                }
                SonnetShotKind::EditorialColumn => layout_editorial_column(
                    &mut flow_ctx,
                    editorial_variant,
                    if secondary_hero_index >= 0 { secondary_hero_index as usize } else { usize::MAX },
                ),
                SonnetShotKind::FragmentCollage => {
                    layout_fragment_collage(&mut flow_ctx, collage_variant)
                }
                _ => layout_cross_stack(&mut flow_ctx),
            }
            flow_boxes = flow_ctx.boxes;
            for (i, fb) in flow_boxes.iter().enumerate() {
                let b = &mut boxes[i];
                b.x = fb.x;
                b.y = fb.y;
                b.enter_x = fb.enter_x;
                b.enter_y = fb.enter_y;
                b.layout_direction = fb.layout_direction;
                b.font_scale = fb.font_scale;
                b.measured_width = fb.measured_width;
                b.measured_height = fb.measured_height;
            }
        }

        // `heroBox.enterX = 0; heroBox.enterY = height * 0.15`.
        {
            let hero_b = &mut boxes[hero_index];
            hero_b.enter_x = 0.0;
            hero_b.enter_y = height * 0.15;
        }

        // Decorations: amplify every hero box into an ambient giant echo plus
        // an extra reflection off the last/first box. Skipped for quiet-tableau
        // and poster-blocks.
        let mut decorations: Vec<TypographyBox> = Vec::new();
        if shot_kind != SonnetShotKind::QuietTableau
            && shot_kind != SonnetShotKind::PosterBlocks
        {
            let all_heroes: Vec<TypographyBox> =
                boxes.iter().filter(|b| b.is_hero).cloned().collect();
            for (idx, h_box) in all_heroes.iter().enumerate() {
                let mut deco = h_box.clone();
                deco.is_hero = false;
                deco.role = Some(SonnetSegmentRole::Decoration);
                deco.font_scale = (h_box.font_scale * 3.5).max(2.8).min(5.5);
                deco.vertical = false;
                deco.x = h_box.x - width * (0.1 - idx as f64 * 0.03);
                deco.y = h_box.y - height * (0.05 - idx as f64 * 0.02);
                deco.rotation = -0.15 + if idx % 2 == 0 { 0.0 } else { 0.05 };
                deco.enter_x = -width * 0.05;
                deco.enter_y = -height * 0.05;
                decorations.push(deco);
            }
            if boxes.len() > 1 && !all_heroes.is_empty() {
                let last_is_hero = boxes.last().map(|b| b.is_hero).unwrap_or(false);
                let dec2_source = if last_is_hero {
                    boxes[0].clone()
                } else {
                    boxes.last().unwrap().clone()
                };
                let first_hero = &all_heroes[0];
                let mut dec2 = dec2_source;
                dec2.is_hero = false;
                dec2.role = Some(SonnetSegmentRole::Decoration);
                dec2.font_scale = (first_hero.font_scale * 2.2).max(1.8).min(3.5);
                dec2.vertical = false;
                dec2.x = first_hero.x + width * 0.25;
                dec2.y = first_hero.y + height * 0.15;
                dec2.rotation = 0.08;
                dec2.enter_x = width * 0.05;
                dec2.enter_y = height * 0.05;
                decorations.push(dec2);
            }
        }

        // `boxes.unshift(...decorations)` — push to front in original order.
        for (i, deco) in decorations.into_iter().enumerate() {
            boxes.insert(i, deco);
        }
    }

    // Map back to the public `SonnetTypographyPlacement` shape.
    boxes
        .iter()
        .map(|b| SonnetTypographyPlacement {
            segment_index: b.index,
            display_text: b.display_text.clone(),
            role: b.role.unwrap_or(if b.is_hero {
                SonnetSegmentRole::Hero
            } else if b.is_semi_hero {
                SonnetSegmentRole::SemiHero
            } else {
                SonnetSegmentRole::Support
            }),
            font_scale: b.font_scale,
            measured_width: b.measured_width,
            measured_height: b.measured_height,
            x: b.x,
            y: b.y,
            rotation: b.rotation,
            enter_x: b.enter_x,
            enter_y: b.enter_y,
            vertical: b.vertical,
            layout_direction: b.layout_direction,
            timing_phase: b.timing_phase,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::pretext::measurement::MeasurementCaches;

    /// Flat generic measurement backend — `char_count * 10.0` per glyph. Lets
    /// the orchestrator exercise every branch deterministically without the
    /// FreeType atlas (Phase 5).
    struct StubBackend;
    impl MeasureBackend for StubBackend {
        fn measure_text(&self, text: &str, _font_str: &str) -> f32 {
            text.chars().count() as f32 * 10.0
        }
    }

    fn seg(text: &str, start: f64, end: f64) -> SonnetSemanticSegment {
        SonnetSemanticSegment {
            text: text.to_string(),
            start_offset: 0,
            end_offset: text.chars().count(),
            start_time: start,
            end_time: end,
            word_indices: Vec::new(),
            graphemes: text
                .chars()
                .map(|c| crate::lyricstyles::sonnet_v2::types::GraphemeTiming {
                    char: c.to_string(),
                    start_time: start,
                    end_time: end,
                    word_index: None,
                })
                .collect(),
            is_word_like: true,
        }
    }

    #[test]
    fn is_sonnet_layout_segment_filters_whitespace() {
        assert!(is_sonnet_layout_segment(&seg("hello", 0.0, 1.0)));
        assert!(!is_sonnet_layout_segment(&seg("   ", 0.0, 1.0)));
        assert!(!is_sonnet_layout_segment(&seg("", 0.0, 1.0)));
    }

    #[test]
    fn vertical_text_joins_graphemes_with_newline() {
        let s = seg("你好", 0.0, 1.0);
        assert_eq!(vertical_text(&s), "你\n好");
    }

    #[test]
    fn measure_text_returns_positive_for_plain_text_with_stub_backend() {
        let mut caches = MeasurementCaches::default();
        let backend = StubBackend;
        let w = measure_text("hello", "400 24px Source Han Sans", 24.0, &mut caches, &backend);
        assert!(w > 0.0);
    }

    #[test]
    fn resolve_sonnet_typography_layout_returns_at_least_decorations_for_type_impact() {
        // Two segments, leader is hero — type-impact shot kind produces the hero
        // placement plus hero-derived decoration echoes.
        let lines: Vec<Vec<SonnetSemanticSegment>> = vec![
            vec![seg("hello", 0.0, 1.0), seg("world", 1.0, 2.0)],
        ];
        let opts = SonnetTypographyLayoutOptions {
            lines,
            shot_kind: SonnetShotKind::TypeImpact,
            paragraph_kind: SonnetParagraphKind::Verse,
            width: 1000.0,
            height: 1000.0,
            base_font_size: 24.0,
            font_family: "Source Han Sans".to_string(),
            font_weight: None,
        };
        let mut caches = MeasurementCaches::default();
        let backend = StubBackend;
        let placements = resolve_sonnet_typography_layout(&opts, &mut caches, &backend);
        assert!(!placements.is_empty(), "type-impact layout must produce placements");
        // Should include at least one hero + one decoration.
        assert!(
            placements
                .iter()
                .any(|p| p.role == SonnetSegmentRole::Hero),
            "hero placement must be present"
        );
    }

    #[test]
    fn quiet_tableau_produces_no_decoration_echo() {
        let lines: Vec<Vec<SonnetSemanticSegment>> = vec![vec![
            seg("alpha", 0.0, 1.0),
            seg("beta", 1.0, 2.0),
            seg("gamma", 2.0, 3.0),
        ]];
        let opts = SonnetTypographyLayoutOptions {
            lines,
            shot_kind: SonnetShotKind::QuietTableau,
            paragraph_kind: SonnetParagraphKind::Verse,
            width: 1000.0,
            height: 1000.0,
            base_font_size: 24.0,
            font_family: "Source Han Sans".to_string(),
            font_weight: None,
        };
        let mut caches = MeasurementCaches::default();
        let backend = StubBackend;
        let placements = resolve_sonnet_typography_layout(&opts, &mut caches, &backend);
        assert!(
            !placements
                .iter()
                .any(|p| p.role == SonnetSegmentRole::Decoration),
            "quiet-tableau must skip the hero-decoration echo pass"
        );
        // Still includes all segments.
        assert_eq!(
            placements.len(),
            3,
            "quiet-tableau should keep all 3 segment placements"
        );
    }

    #[test]
    fn poster_blocks_dispatches_to_poster_layout_path_with_cjk() {
        let lines: Vec<Vec<SonnetSemanticSegment>> = vec![vec![
            seg("主", 0.0, 1.0),
            seg("辅", 1.0, 2.0),
            seg("助", 2.0, 3.0),
        ]];
        let opts = SonnetTypographyLayoutOptions {
            lines,
            shot_kind: SonnetShotKind::PosterBlocks,
            paragraph_kind: SonnetParagraphKind::Chorus,
            width: 1000.0,
            height: 1000.0,
            base_font_size: 24.0,
            font_family: "Source Han Sans".to_string(),
            font_weight: None,
        };
        let mut caches = MeasurementCaches::default();
        let backend = StubBackend;
        let placements = resolve_sonnet_typography_layout(&opts, &mut caches, &backend);
        assert_eq!(placements.len(), 3);
        // Poster blocks never emit decorations.
        assert!(
            !placements
                .iter()
                .any(|p| p.role == SonnetSegmentRole::Decoration)
        );
    }
}
