//! Folia sonnet v2 — pretext `rich-inline.ts` (518 lines) `compiler-grade 1:1 port`.
//!
//! Helper for rich-text inline flow under `white-space: normal`. It keeps the
//! core layout API low-level while taking over the boring shared work that
//! rich inline demos kept reimplementing in userland:
//! - collapsed boundary whitespace across item boundaries
//! - atomic inline boxes like pills
//! - per-item extra horizontal chrome such as padding/borders
//!
//! See `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` Phase 2.8.

use std::cell::RefCell;

use crate::lyricstyles::sonnet_v2::pretext::{
    layout, line_break, line_text, measurement,
};

use layout::{LayoutCursor, PreparedTextWithSegments};
use line_break::{LineBreakCursor, step_prepared_line_geometry};
use measurement::{MeasureBackend, MeasurementCaches};

// ===== Public types (byte-faithful to TS) =====

/// `RichInlineItem` — pretext rich-inline.ts.
pub struct RichInlineItem<'a> {
    /// Raw author text, including any leading/trailing collapsible spaces.
    pub text: &'a str,
    /// Canvas font shorthand used to prepare and measure this item.
    pub font: &'a str,
    /// Extra horizontal spacing between graphemes, in CSS px. `None` == 0.
    pub letter_spacing: Option<f32>,
    /// `'never'` keeps the item atomic, like a pill or mention chip. `None` ==
    /// `'normal'`.
    pub break_mode: Option<RichInlineBreak>,
    /// Caller-owned horizontal chrome, e.g. padding + border width. `None` == 0.
    pub extra_width: Option<f32>,
}

/// `RichInlineItem['break']` — pretext rich-inline.ts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RichInlineBreak {
    Normal,
    Never,
}

/// `PreparedTextWithSegments` brand-equivalent. Rust replaces the TS nominal
/// brand with a concrete type.
pub struct PreparedRichInline {
    pub(crate) items: Vec<PreparedRichInlineItem>,
    /// Lookup by original source `RichInlineItem` array index (sparse; gap
    /// entries are `None` when the item dropped out — empty text or single-line
    /// measurement failure).
    pub(crate) items_by_source_item_index: Vec<Option<PreparedRichInlineItem>>,
}

/// `RichInlineCursor` — pretext rich-inline.ts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RichInlineCursor {
    pub item_index: usize,
    pub segment_index: usize,
    pub grapheme_index: usize,
}

/// `RichInlineFragment` — rich-inline.ts. Carries the materialized text slice.
#[derive(Clone, Debug)]
pub struct RichInlineFragment {
    pub item_index: usize,
    pub text: String,
    pub gap_before: f32,
    pub occupied_width: f32,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}

/// `RichInlineFragmentRange` — rich-inline.ts. The cheap shrinkwrap/probe shape
/// (no text yet).
#[derive(Clone, Debug)]
pub struct RichInlineFragmentRange {
    pub item_index: usize,
    pub gap_before: f32,
    pub occupied_width: f32,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}

/// `RichInlineLine` — rich-inline.ts.
#[derive(Clone, Debug)]
pub struct RichInlineLine {
    pub fragments: Vec<RichInlineFragment>,
    pub width: f32,
    pub end: RichInlineCursor,
}

/// `RichInlineLineRange` — rich-inline.ts.
#[derive(Clone, Debug)]
pub struct RichInlineLineRange {
    pub fragments: Vec<RichInlineFragmentRange>,
    pub width: f32,
    pub end: RichInlineCursor,
}

/// `RichInlineStats` — rich-inline.ts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichInlineStats {
    pub line_count: usize,
    pub max_line_width: f32,
}

// ===== Internal prepared-item type =====

/// Item in a `PreparedRichInline`. Tied by source item index back to the
/// caller's `RichInlineItem` array.
#[derive(Clone, Debug)]
pub(crate) struct PreparedRichInlineItem {
    pub break_mode: RichInlineBreak,
    pub end_grapheme_index: usize,
    pub end_segment_index: usize,
    pub extra_width: f32,
    pub gap_before: f32,
    pub natural_width: f32,
    pub prepared: PreparedTextWithSegments,
    pub source_item_index: usize,
}

impl PreparedRichInlineItem {
    fn clone_for_fragment(&self) -> Self {
        // Only the metadata needed by the fragment collector; the prepared
        // text is not actually borrowed by collectors (TS passes the whole item
        // reference) — but we clone the metadata so the borrowed item can be
        // dropped later when materializing rich-line text.
        Self {
            break_mode: self.break_mode,
            end_grapheme_index: self.end_grapheme_index,
            end_segment_index: self.end_segment_index,
            extra_width: self.extra_width,
            gap_before: self.gap_before,
            natural_width: self.natural_width,
            prepared: self.prepared.clone(),
            source_item_index: self.source_item_index,
        }
    }
}

// ===== Module-private helpers =====

const EMPTY_LAYOUT_CURSOR: LayoutCursor = LayoutCursor { segment_index: 0, grapheme_index: 0 };
const RICH_INLINE_START_CURSOR: RichInlineCursor = RichInlineCursor {
    item_index: 0,
    segment_index: 0,
    grapheme_index: 0,
};

fn is_collapsible_boundary_whitespace(text: &str) -> bool {
    // TS `[ \t\n\f\r]+` test — true iff at least one such char present.
    text.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\u{000c}' | '\r'))
}

fn leading_collapsible_boundary(text: &str) -> bool {
    text.chars().next().map_or(false, |c| matches!(c, ' ' | '\t' | '\n' | '\u{000c}' | '\r'))
}

fn trailing_collapsible_boundary(text: &str) -> bool {
    text.chars().next_back().map_or(false, |c| matches!(c, ' ' | '\t' | '\n' | '\u{000c}' | '\r'))
}

fn trim_collapsible_boundaries(text: &str) -> &str {
    text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{000c}' | '\r'))
}

fn is_line_start_cursor(cursor: &LayoutCursor) -> bool {
    cursor.segment_index == 0 && cursor.grapheme_index == 0
}

/// `getCollapsedSpaceWidth` — rich-inline.ts.
fn get_collapsed_space_width(
    backend: &impl MeasureBackend,
    caches: &mut measurement::MeasurementCaches,
    font: &str,
    letter_spacing: f32,
    cache: &RefCell<std::collections::HashMap<String, f32>>,
) -> f32 {
    // Key: `${font}\u0000${letterSpacing}`.
    let cache_key = format!("{}\u{0}\u{0}\u{0}{}", font, letter_spacing);
    if let Some(cached) = cache.borrow().get(&cache_key) {
        return *cached;
    }
    let options = layout::PrepareOptions {
        letter_spacing,
        ..Default::default()
    };
    // TS uses prepareWithSegments('A A', font, …) vs ('AA', font, …); we must
    // use the same backend the rich-inline items themselves will be measured
    // with. Reuse a thread-local cache slot inside the caller-supplied caches
    // so it survives repeated calls inside prepareRichInline.
    let joined_width = {
        let prepared = layout::prepare_with_segments("A A", caches, backend, font, options.clone());
        layout::measure_natural_width(&prepared)
    };
    let compact_width = {
        let prepared = layout::prepare_with_segments("AA", caches, backend, font, options);
        layout::measure_natural_width(&prepared)
    };
    let collapsed = (joined_width - compact_width).max(0.0);
    cache.borrow_mut().insert(cache_key, collapsed);
    collapsed
}

/// `prepareWholeItemLine` — rich-inline.ts. Returns the line geometry that fits
/// the whole item on one line at `+Infinity` width.
fn prepare_whole_item_line(
    prepared: &PreparedTextWithSegments,
) -> Option<PreparedWholeItemLine> {
    let mut end: LineBreakCursor = LineBreakCursor { segment_index: 0, grapheme_index: 0 };
    // TS uses `Number.POSITIVE_INFINITY`; our stepper clamps `max_width` once
    // used only for fit comparison — passing `f32::INFINITY` matches the TS
    // "always fits" semantics.
    let width = step_prepared_line_geometry(&prepared.prepared, &mut end, f32::INFINITY)?;
    Some(PreparedWholeItemLine {
        end_grapheme_index: end.grapheme_index,
        end_segment_index: end.segment_index,
        width,
    })
}

struct PreparedWholeItemLine {
    end_grapheme_index: usize,
    end_segment_index: usize,
    width: f32,
}

/// `endsInsideFirstSegment` — rich-inline.ts.
fn ends_inside_first_segment(segment_index: usize, grapheme_index: usize) -> bool {
    segment_index == 0 && grapheme_index > 0
}

// ===== Public API: prepare =====

/// `prepareRichInline` — pretext rich-inline.ts. The standalone TS version
/// shares a single `Intl.Segmenter` / DOM-canvas measurement scope across all
/// items. The Rust port threads an explicit `backend` + re-entrant `caches`
/// through each `prepareWithSegments` so Folia's per-item measurement is
/// byte-faithful without relying on browser globals.
pub fn prepare_rich_inline<'a, B: MeasureBackend>(
    items: impl IntoIterator<Item = RichInlineItem<'a>>,
    backend: &B,
    caches: &mut measurement::MeasurementCaches,
) -> PreparedRichInline {
    let items: Vec<RichInlineItem<'a>> = items.into_iter().collect();
    let mut prepared_items: Vec<PreparedRichInlineItem> = Vec::new();
    let mut items_by_source = vec![None; items.len()];
    let collapsed_space_width_cache = RefCell::new(std::collections::HashMap::<String, f32>::new());
    let mut pending_gap_width: f32 = 0.0;

    for (index, item) in items.into_iter().enumerate() {
        let letter_spacing = item.letter_spacing.unwrap_or(0.0);
        let has_leading_whitespace = leading_collapsible_boundary(item.text);
        let has_trailing_whitespace = trailing_collapsible_boundary(item.text);
        let trimmed_text = trim_collapsible_boundaries(item.text);

        if trimmed_text.is_empty() {
            if is_collapsible_boundary_whitespace(item.text) && pending_gap_width == 0.0 {
                pending_gap_width = get_collapsed_space_width(
                    backend, &mut *caches, item.font, letter_spacing, &collapsed_space_width_cache,
                );
            }
            continue;
        }

        let gap_before = if pending_gap_width > 0.0 {
            pending_gap_width
        } else if has_leading_whitespace {
            get_collapsed_space_width(
                backend, &mut *caches, item.font, letter_spacing, &collapsed_space_width_cache,
            )
        } else {
            0.0
        };

        let prepared = layout::prepare_with_segments(
            trimmed_text,
            caches,
            backend,
            item.font,
            layout::PrepareOptions {
                letter_spacing,
                ..Default::default()
            },
        );

        let whole_line = match prepare_whole_item_line(&prepared) {
            Some(w) => w,
            None => {
                pending_gap_width = if has_trailing_whitespace {
                    get_collapsed_space_width(
                        backend, caches, item.font, letter_spacing, &collapsed_space_width_cache,
                    )
                } else {
                    0.0
                };
                continue;
            }
        };

        let prepared_item = PreparedRichInlineItem {
            break_mode: item.break_mode.unwrap_or(RichInlineBreak::Normal),
            end_grapheme_index: whole_line.end_grapheme_index,
            end_segment_index: whole_line.end_segment_index,
            extra_width: item.extra_width.unwrap_or(0.0),
            gap_before,
            natural_width: whole_line.width,
            prepared,
            source_item_index: index,
        };
        prepared_items.push(prepared_item.clone_for_fragment());
        // Stable borrow of whoever lands in prepared_items slot for `index`.
        // Use a clone for the by-index lookup so caller doesn't share lifetime
        // with the dense vec.
        items_by_source[index] = Some(prepared_item);

        pending_gap_width = if has_trailing_whitespace {
            get_collapsed_space_width(
                backend, &mut *caches, item.font, letter_spacing, &collapsed_space_width_cache,
            )
        } else {
            0.0
        };
    }

    PreparedRichInline {
        items: prepared_items,
        items_by_source_item_index: items_by_source,
    }
}

// ===== Internal line walker =====

/// `stepRichInlineLine` — pretext rich-inline.ts. Steady-borrow walker over a
/// `PreparedRichInline`. Returns the laid-out line width (`None` if no
/// progress). Mutates `cursor` to the start of the next line.
///
/// `collect_fragment` is invoked once per item fragment emitted on this line,
/// in TS call order (so the parse matches byte-for-byte).
pub(crate) fn step_rich_inline_line<F>(
    flow: &PreparedRichInline,
    max_width: f32,
    cursor: &mut RichInlineCursor,
    mut collect_fragment: Option<F>,
) -> Option<f32>
where
    F: FnMut(&PreparedRichInlineItem, f32, f32, LayoutCursor, LayoutCursor),
{
    if flow.items.is_empty() || cursor.item_index >= flow.items.len() {
        return None;
    }

    let safe_width = max_width.max(1.0);
    let mut line_width: f32 = 0.0;
    let mut remaining_width: f32 = safe_width;
    let mut item_index = cursor.item_index;

    'line_loop: loop {
        if item_index >= flow.items.len() {
            break;
        }
        let item = &flow.items[item_index];

        if !is_line_start_cursor(&LayoutCursor {
            segment_index: cursor.segment_index,
            grapheme_index: cursor.grapheme_index,
        }) && cursor.segment_index == item.end_segment_index
            && cursor.grapheme_index == item.end_grapheme_index
        {
            item_index += 1;
            cursor.segment_index = 0;
            cursor.grapheme_index = 0;
            continue;
        }

        let gap_before = if line_width == 0.0 { 0.0 } else { item.gap_before };
        let at_item_start = is_line_start_cursor(&LayoutCursor {
            segment_index: cursor.segment_index,
            grapheme_index: cursor.grapheme_index,
        });

        if item.break_mode == RichInlineBreak::Never {
            if !at_item_start {
                item_index += 1;
                cursor.segment_index = 0;
                cursor.grapheme_index = 0;
                continue;
            }
            let occupied_width = item.natural_width + item.extra_width;
            let total_width = gap_before + occupied_width;
            if line_width > 0.0 && total_width > remaining_width {
                break 'line_loop;
            }
            if let Some(collect) = collect_fragment.as_mut() {
                collect(
                    item,
                    gap_before,
                    occupied_width,
                    EMPTY_LAYOUT_CURSOR,
                    LayoutCursor {
                        segment_index: item.end_segment_index,
                        grapheme_index: item.end_grapheme_index,
                    },
                );
            }
            line_width += total_width;
            remaining_width = (safe_width - line_width).max(0.0);
            item_index += 1;
            cursor.segment_index = 0;
            cursor.grapheme_index = 0;
            continue;
        }

        let reserved_width = gap_before + item.extra_width;
        if line_width > 0.0 && reserved_width >= remaining_width {
            break 'line_loop;
        }

        if at_item_start {
            let total_width = reserved_width + item.natural_width;
            if total_width <= remaining_width {
                if let Some(collect) = collect_fragment.as_mut() {
                    collect(
                        item,
                        gap_before,
                        item.natural_width + item.extra_width,
                        EMPTY_LAYOUT_CURSOR,
                        LayoutCursor {
                            segment_index: item.end_segment_index,
                            grapheme_index: item.end_grapheme_index,
                        },
                    );
                }
                line_width += total_width;
                remaining_width = (safe_width - line_width).max(0.0);
                item_index += 1;
                cursor.segment_index = 0;
                cursor.grapheme_index = 0;
                continue;
            }
        }

        let available_width = (remaining_width - reserved_width).max(1.0);
        let mut line_end = LineBreakCursor {
            segment_index: cursor.segment_index,
            grapheme_index: cursor.grapheme_index,
        };
        let line_width_for_item =
            step_prepared_line_geometry(&item.prepared.prepared, &mut line_end, available_width);
        let line_width_for_item = match line_width_for_item {
            Some(w) => w,
            None => {
                item_index += 1;
                cursor.segment_index = 0;
                cursor.grapheme_index = 0;
                continue;
            }
        };
        if cursor.segment_index == line_end.segment_index
            && cursor.grapheme_index == line_end.grapheme_index
        {
            item_index += 1;
            cursor.segment_index = 0;
            cursor.grapheme_index = 0;
            continue;
        }

        let item_occupied_width = line_width_for_item + item.extra_width;
        let line_width_contribution = gap_before + item_occupied_width;

        // The lower-level walker may force one unit to make progress. If that
        // unit only fits on a fresh line, wrap before this rich item instead.
        if line_width > 0.0 && at_item_start && line_width_contribution > remaining_width {
            break 'line_loop;
        }

        // If the only thing we can fit after paying the boundary gap is a
        // partial slice of the item's first segment, prefer wrapping before
        // the item so we keep whole-word-style boundaries when they exist. But
        // once the current line can consume a real breakable unit from the
        // item, stay greedy and keep filling the line.
        if line_width > 0.0 && at_item_start && gap_before > 0.0
            && ends_inside_first_segment(line_end.segment_index, line_end.grapheme_index)
        {
            let mut fresh_line_end = LineBreakCursor { segment_index: 0, grapheme_index: 0 };
            let fresh_line_width = step_prepared_line_geometry(
                &item.prepared.prepared,
                &mut fresh_line_end,
                (safe_width - item.extra_width).max(1.0),
            );
            if let Some(fw) = fresh_line_width {
                let fresh_advances =
                    fresh_line_end.segment_index > line_end.segment_index
                    || (fresh_line_end.segment_index == line_end.segment_index
                        && fresh_line_end.grapheme_index > line_end.grapheme_index);
                if fresh_advances {
                    break 'line_loop;
                }
            }
        }

        if let Some(collect) = collect_fragment.as_mut() {
            collect(
                item,
                gap_before,
                item_occupied_width,
                LayoutCursor {
                    segment_index: cursor.segment_index,
                    grapheme_index: cursor.grapheme_index,
                },
                LayoutCursor {
                    segment_index: line_end.segment_index,
                    grapheme_index: line_end.grapheme_index,
                },
            );
        }
        line_width += line_width_contribution;
        remaining_width = (safe_width - line_width).max(0.0);

        if line_end.segment_index == item.end_segment_index
            && line_end.grapheme_index == item.end_grapheme_index
        {
            item_index += 1;
            cursor.segment_index = 0;
            cursor.grapheme_index = 0;
            continue;
        }

        cursor.segment_index = line_end.segment_index;
        cursor.grapheme_index = line_end.grapheme_index;
        break;
    }

    if line_width == 0.0 {
        return None;
    }
    cursor.item_index = item_index;
    Some(line_width)
}

// ===== Public range walker + materializer =====

/// `layoutNextRichInlineLineRange` — pretext rich-inline.ts.
pub fn layout_next_rich_inline_line_range(
    prepared: &PreparedRichInline,
    max_width: f32,
    start: RichInlineCursor,
) -> Option<RichInlineLineRange> {
    let mut end = RichInlineCursor {
        item_index: start.item_index,
        segment_index: start.segment_index,
        grapheme_index: start.grapheme_index,
    };
    let mut fragments: Vec<RichInlineFragmentRange> = Vec::new();
    let width = step_rich_inline_line(prepared, max_width, &mut end, Some(
        |_item: &PreparedRichInlineItem,
         gap_before: f32,
         occupied_width: f32,
         fragment_start: LayoutCursor,
         fragment_end: LayoutCursor| {
            fragments.push(RichInlineFragmentRange {
                item_index: _item.source_item_index,
                gap_before,
                occupied_width,
                start: fragment_start,
                end: fragment_end,
            });
        },
    ))?;
    Some(RichInlineLineRange { fragments, width, end })
}

/// `materializeFragmentText` — rich-inline.ts. Builds the visible text slice
/// for one fragment from its source item's prepared view + a line-text cache.
fn materialize_fragment_text(
    item: &PreparedRichInlineItem,
    fragment: &RichInlineFragmentRange,
) -> String {
    let cache = line_text::get_line_text_cache();
    line_text::build_line_text_from_range(
        &item.prepared.as_view(),
        &cache,
        fragment.start.segment_index,
        fragment.start.grapheme_index,
        fragment.end.segment_index,
        fragment.end.grapheme_index,
    )
}

/// `materializeRichInlineLineRange` — pretext rich-inline.ts. Bridge from
/// cheap range walking to full fragment text. Lets callers do
/// shrinkwrap/virtualization/probing work first, then only pay for text on the
/// lines they actually render.
pub fn materialize_rich_inline_line_range(
    prepared: &PreparedRichInline,
    line: &RichInlineLineRange,
) -> RichInlineLine {
    let mut fragments: Vec<RichInlineFragment> = Vec::new();
    for fragment in &line.fragments {
        let item = prepared
            .items_by_source_item_index
            .get(fragment.item_index)
            .and_then(|opt| opt.as_ref())
            .expect("Missing rich-text inline item for fragment");
        fragments.push(RichInlineFragment {
            item_index: fragment.item_index,
            text: materialize_fragment_text(item, fragment),
            gap_before: fragment.gap_before,
            occupied_width: fragment.occupied_width,
            start: fragment.start,
            end: fragment.end,
        });
    }
    RichInlineLine {
        fragments,
        width: line.width,
        end: line.end,
    }
}

/// `walkRichInlineLineRanges` — pretext rich-inline.ts.
pub fn walk_rich_inline_line_ranges<F>(prepared: &PreparedRichInline, max_width: f32, mut on_line: F) -> usize
where
    F: FnMut(&RichInlineLineRange),
{
    let mut line_count = 0usize;
    let mut cursor = RICH_INLINE_START_CURSOR;
    loop {
        let line = match layout_next_rich_inline_line_range(prepared, max_width, cursor) {
            Some(l) => l,
            None => return line_count,
        };
        on_line(&line);
        line_count += 1;
        cursor = line.end;
    }
}

/// `measureRichInlineStats` — pretext rich-inline.ts.
pub fn measure_rich_inline_stats(prepared: &PreparedRichInline, max_width: f32) -> RichInlineStats {
    let mut line_count = 0usize;
    let mut max_line_width = 0.0_f32;
    let mut cursor = RichInlineCursor { item_index: 0, segment_index: 0, grapheme_index: 0 };
    loop {
        let line_width = match step_rich_inline_line::<fn(&PreparedRichInlineItem, f32, f32, LayoutCursor, LayoutCursor)>(
            prepared, max_width, &mut cursor, None,
        ) {
            Some(w) => w,
            None => return RichInlineStats { line_count, max_line_width },
        };
        line_count += 1;
        if line_width > max_line_width {
            max_line_width = line_width;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ByteLenBackend;
    impl MeasureBackend for ByteLenBackend {
        fn measure_text(&self, text: &str, _font_str: &str) -> f32 {
            text.chars().count() as f32
        }
    }

    fn mk_items<'a>(texts: &'a [&'a str]) -> Vec<RichInlineItem<'a>> {
        texts
            .iter()
            .map(|t| RichInlineItem {
                text: t,
                font: "16px sans-serif",
                letter_spacing: None,
                break_mode: None,
                extra_width: None,
            })
            .collect()
    }

    #[test]
    fn prepare_rich_inline_drops_empty_items_but_leaves_a_pending_gap() {
        let backend = ByteLenBackend;
        let mut caches = measurement::MeasurementCaches::default();
        let items = mk_items(&["hello", "   ", "world"]);
        let prepared = prepare_rich_inline(items, &backend, &mut caches);
        // The middle whitespace-only item stores a `gapBefore` on `world`.
        assert_eq!(prepared.items.len(), 2, "expected two non-empty items");
        assert_eq!(prepared.items[0].source_item_index, 0);
        assert_eq!(prepared.items[1].source_item_index, 2);
        assert!(prepared.items[1].gap_before > 0.0, "world should carry the collapsed gap");
        // by-source lookup keeps the missing index as None.
        assert!(prepared.items_by_source_item_index[1].is_none());
    }

    #[test]
    fn layout_rich_inline_folds_into_one_line_when_width_allows() {
        let backend = ByteLenBackend;
        let mut caches = measurement::MeasurementCaches::default();
        let items = mk_items(&["hello", " ", "world"]);
        let prepared = prepare_rich_inline(items, &backend, &mut caches);
        // "hello world" is 11 chars wide; generous width fits in one line.
        let line = layout_next_rich_inline_line_range(&prepared, 100.0, RICH_INLINE_START_CURSOR)
            .expect("expected at least one line");
        assert_eq!(line.fragments.len(), 2, "both items on one line");
        assert!(line.width > 0.0);
        assert!(line.end.item_index >= 2);
    }

    #[test]
    fn walk_rich_inline_emits_each_line_range() {
        let backend = ByteLenBackend;
        let mut caches = measurement::MeasurementCaches::default();
        let items = mk_items(&["aaa bbb ccc", "x", "yy zzz"]);
        let prepared = prepare_rich_inline(items, &backend, &mut caches);
        let mut count = 0;
        let total = walk_rich_inline_line_ranges(&prepared, 3.0, |_| count += 1);
        assert_eq!(count, total, "callback count matches returned total");
        assert!(total > 1, "expected folding at width 3.0");
    }

    #[test]
    fn materialize_line_with_two_fragments_roundtrips_text() {
        let backend = ByteLenBackend;
        let mut caches = measurement::MeasurementCaches::default();
        let items = mk_items(&["hello", " ", "world"]);
        let prepared = prepare_rich_inline(items, &backend, &mut caches);
        let range = layout_next_rich_inline_line_range(&prepared, 100.0, RICH_INLINE_START_CURSOR)
            .expect("expected one line");
        let line = materialize_rich_inline_line_range(&prepared, &range);
        assert_eq!(line.fragments.len(), 2);
        // Both fragments should round-trip their source text.
        assert_eq!(line.fragments[0].text, "hello");
        assert_eq!(line.fragments[1].text, "world");
    }
}
