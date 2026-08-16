//! Folia sonnet v2 — pretext `layout.ts` (914 lines) `compiler-grade 1:1 port`.
//!
//! Two-phase text measurement: `prepare` segments + measures once, `layout`
//! walks cached widths with pure arithmetic. The TS source uses
//! `Intl.Segmenter('grapheme')` for grapheme-level indexing; this is replaced
//! with `unicode_segmentation::UnicodeSegmentation::grapheme_indices` (UAX #29
//! cluster boundaries), byte-identical for the .ts codebase's grapheme inputs.
//!
//! See `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` Phase 2.7.

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::lyricstyles::sonnet_v2::pretext::analysis::{
    AnalysisChunk, AnalysisProfile, SegmentBreakKind, TextAnalysis, WhiteSpaceMode, WordBreakMode,
    analyze_text, can_continue_keep_all_text_run, ends_with_closing_quote, is_cjk,
    is_numeric_run_segment, KINSOKU_END, KINSOKU_START, LEFT_STICKY_PUNCTUATION,
};
use crate::lyricstyles::sonnet_v2::pretext::bidi::compute_segment_levels;
use crate::lyricstyles::sonnet_v2::pretext::line_break::{
    LineBreakCursor as LbCursor, LineGeometryStats, PreparedLineBreakData, count_prepared_lines,
    measure_prepared_line_geometry, normalize_prepared_line_start,
    step_prepared_line_geometry_from_chunk, walk_prepared_lines_raw,
};
use crate::lyricstyles::sonnet_v2::pretext::line_text::{
    LineTextCache, PreparedSegmentView, build_line_text_from_range, clear_line_text_caches,
    get_line_text_cache,
};
use crate::lyricstyles::sonnet_v2::pretext::measurement::{
    BreakableFitMode, EngineProfile, FontMeasurementState, MeasurementCaches, MeasureBackend,
    get_corrected_segment_width, get_engine_profile, get_font_measurement_state,
    text_may_contain_emoji,
};

// ===== Public option/result types =====

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCursor {
    pub segment_index: usize,
    pub grapheme_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutResult {
    pub line_count: usize,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineStats {
    pub line_count: usize,
    pub max_line_width: f32,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutLine {
    pub text: String,
    pub width: f32,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutLineRange {
    pub width: f32,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutLinesResult {
    pub line_count: usize,
    pub height: f32,
    pub lines: Vec<LayoutLine>,
}

#[derive(Clone, Copy, Debug)]
pub struct PrepareOptions {
    pub white_space: WhiteSpaceMode,
    pub word_break: WordBreakMode,
    pub letter_spacing: f32,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            white_space: WhiteSpaceMode::Normal,
            word_break: WordBreakMode::Normal,
            letter_spacing: 0.0,
        }
    }
}

// ===== Prepared-handle structs =====
//
// `PreparedText` is the compact height-prediction handle returned from
// `prepare`. `PreparedTextWithSegments` is the rich variant that additionally
// owns the per-segment text slice + bidi metadata so callers can render the
// laid-out lines themselves. Both wrap the same `PreparedLineBreakData`
// snapshot owned by the line walker; `PreparedText` simply hides the
// segment/seg_levels fields (byte-faithful with the TS opaque branded type).

#[derive(Clone, Debug)]
pub struct PreparedText {
    pub(crate) prepared: PreparedLineBreakData,
    pub(crate) seg_levels: Option<Vec<i8>>,
}

#[derive(Clone, Debug)]
pub struct PreparedTextWithSegments {
    pub(crate) prepared: PreparedLineBreakData,
    pub(crate) seg_levels: Option<Vec<i8>>,
    pub segments: Vec<String>,
}

impl PreparedTextWithSegments {
    fn as_view(&self) -> PreparedSegmentView<'_> {
        PreparedSegmentView {
            segments: &self.segments,
            kinds: &self.prepared.kinds,
        }
    }
}

fn empty_prepared_break_data() -> PreparedLineBreakData {
    PreparedLineBreakData {
        widths: vec![],
        line_end_fit_advances: vec![],
        line_end_paint_advances: vec![],
        kinds: vec![],
        simple_line_walk_fast_path: true,
        breakable_fit_advances: vec![],
        breakable_preferred_breaks: vec![],
        letter_spacing: 0.0,
        spacing_grapheme_counts: vec![],
        discretionary_hyphen_width: 0.0,
        tab_stop_advance: 0.0,
        chunks: vec![],
    }
}

fn create_empty_prepared(include_segments: bool) -> PreparedTextWithSegments {
    let _ = include_segments;
    PreparedTextWithSegments {
        prepared: empty_prepared_break_data(),
        seg_levels: None,
        segments: vec![],
    }
}

// ===== Build helpers (CJK unit grouping + keep-all merge + letter spacing) =====
//
// `buildBaseCjkUnits` walks graphemes of a CJK-bearing text segment and
// accumulates them into "units" — runs glued together by kinsoku / closing
// quote / left-sticky-punctuation rules. `mergeKeepAllTextUnits` then collapses
// keep-all text runs across units so the walker treats one CJK phrase as one
// unbreakable segment.

#[derive(Clone)]
struct MeasuredTextUnit {
    text: String,
    start: usize,
}

struct CjkUnitBuilder<'a> {
    seg_text: &'a str,
    engine_profile: EngineProfile,
    units: Vec<MeasuredTextUnit>,
    // current unit accumulator
    parts: Vec<String>,
    start: usize,
    contains_cjk: bool,
    ends_with_closing_quote: bool,
    is_single_kinsoku_end: bool,
}

impl<'a> CjkUnitBuilder<'a> {
    fn new(seg_text: &'a str, engine_profile: EngineProfile) -> Self {
        Self {
            seg_text,
            engine_profile,
            units: vec![],
            parts: vec![],
            start: 0,
            contains_cjk: false,
            ends_with_closing_quote: false,
            is_single_kinsoku_end: false,
        }
    }

    fn push_unit(&mut self) {
        if self.parts.is_empty() {
            return;
        }
        let text = if self.parts.len() == 1 {
            self.parts[0].clone()
        } else {
            self.parts.concat()
        };
        self.units.push(MeasuredTextUnit { text, start: self.start });
        self.parts.clear();
        self.contains_cjk = false;
        self.ends_with_closing_quote = false;
        self.is_single_kinsoku_end = false;
    }

    fn start_unit(&mut self, grapheme: String, start: usize, grapheme_contains_cjk: bool) {
        self.parts = vec![grapheme];
        self.start = start;
        self.contains_cjk = grapheme_contains_cjk;
        self.ends_with_closing_quote = ends_with_closing_quote(&self.parts[0]);
        self.is_single_kinsoku_end = KINSOKU_END.contains(&self.parts[0].chars().next().unwrap_or('\0'));
    }

    fn append_to_unit(&mut self, grapheme: String, grapheme_contains_cjk: bool) {
        let len1 = grapheme.chars().count() == 1;
        let first = grapheme.chars().next().unwrap_or('\0');
        let grapheme_ends_with_closing_quote = ends_with_closing_quote(&grapheme);
        self.parts.push(grapheme);
        self.contains_cjk = self.contains_cjk || grapheme_contains_cjk;
        if len1 && LEFT_STICKY_PUNCTUATION.contains(&first) {
            self.ends_with_closing_quote =
                self.ends_with_closing_quote || grapheme_ends_with_closing_quote;
        } else {
            self.ends_with_closing_quote = grapheme_ends_with_closing_quote;
        }
        self.is_single_kinsoku_end = false;
    }

    fn walk(mut self) -> Vec<MeasuredTextUnit> {
        for (gs_start, grapheme) in UnicodeSegmentation::grapheme_indices(self.seg_text, true) {
            let grapheme_contains_cjk = is_cjk(grapheme);
            if self.parts.is_empty() {
                self.start_unit(grapheme.to_string(), gs_start, grapheme_contains_cjk);
                continue;
            }
            let first = grapheme.chars().next().unwrap_or('\0');
            if self.is_single_kinsoku_end
                || KINSOKU_START.contains(&first)
                || LEFT_STICKY_PUNCTUATION.contains(&first)
                || (self.engine_profile.carry_cjk_after_closing_quote
                    && grapheme_contains_cjk
                    && self.ends_with_closing_quote)
            {
                self.append_to_unit(grapheme.to_string(), grapheme_contains_cjk);
                continue;
            }
            if !self.contains_cjk && !grapheme_contains_cjk {
                self.append_to_unit(grapheme.to_string(), grapheme_contains_cjk);
                continue;
            }
            self.push_unit();
            self.start_unit(grapheme.to_string(), gs_start, grapheme_contains_cjk);
        }
        self.push_unit();
        self.units
    }
}

fn build_base_cjk_units(seg_text: &str, engine_profile: EngineProfile) -> Vec<MeasuredTextUnit> {
    CjkUnitBuilder::new(seg_text, engine_profile).walk()
}

fn merge_keep_all_text_units(
    seg_text: &str,
    units: Vec<MeasuredTextUnit>,
    break_after_punctuation: bool,
) -> Vec<MeasuredTextUnit> {
    // TS short-circuits when units.len() <= 1; the merge loop below preserves
    // that fast path verbatim.
    if units.len() <= 1 {
        return units;
    }
    let units_len = units.len();
    let mut merged: Vec<MeasuredTextUnit> = vec![];
    let mut group_start: i64 = -1;
    let mut group_contains_cjk = false;

    let flush_group = |merged: &mut Vec<MeasuredTextUnit>,
                       units: &[MeasuredTextUnit],
                       group_start: &mut i64,
                       end: usize,
                       seg_text: &str,
                       group_contains_cjk: &mut bool| {
        if *group_start < 0 {
            return;
        }
        let gs = *group_start as usize;
        if *group_contains_cjk {
            if gs + 1 == end {
                merged.push(units[gs].clone());
            } else {
                let source_start = units[gs].start;
                let source_end = if end < units.len() { units[end].start } else { seg_text.len() };
                merged.push(MeasuredTextUnit {
                    text: seg_text[source_start..source_end].to_string(),
                    start: source_start,
                });
            }
        } else {
            for i in gs..end {
                merged.push(units[i].clone());
            }
        }
        *group_start = -1;
        *group_contains_cjk = false;
    };

    for i in 0..units_len {
        let unit = &units[i];
        if group_start >= 0
            && !can_continue_keep_all_text_run(&units[i - 1].text, break_after_punctuation)
        {
            flush_group(&mut merged, &units, &mut group_start, i, seg_text, &mut group_contains_cjk);
        }
        if group_start < 0 {
            group_start = i as i64;
        }
        group_contains_cjk = group_contains_cjk || is_cjk(&unit.text);
    }

    flush_group(&mut merged, &units, &mut group_start, units_len, seg_text, &mut group_contains_cjk);
    merged
}

fn count_rendered_spacing_graphemes(text: &str, kind: SegmentBreakKind) -> usize {
    if matches!(
        kind,
        SegmentBreakKind::ZeroWidthBreak | SegmentBreakKind::SoftHyphen | SegmentBreakKind::HardBreak
    ) {
        return 0;
    }
    if matches!(kind, SegmentBreakKind::Tab) {
        return 1;
    }
    UnicodeSegmentation::graphemes(text, true).count()
}

fn is_preferred_break_grapheme(grapheme: &str) -> bool {
    grapheme == "-"
        || grapheme == "\u{058a}"
        || grapheme == "\u{2010}"
        || grapheme == "\u{2012}"
        || grapheme == "\u{2013}"
        || grapheme == "\u{2014}"
}

fn get_breakable_preferred_breaks(text: &str) -> Option<Vec<usize>> {
    if !text.chars().any(|c| {
        c == '-'
            || c == '\u{058a}'
            || c == '\u{2010}'
            || c == '\u{2012}'
            || c == '\u{2013}'
            || c == '\u{2014}'
    }) {
        return None;
    }
    let mut breaks: Vec<usize> = vec![];
    let mut grapheme_index = 0usize;
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        grapheme_index += 1;
        if is_preferred_break_grapheme(grapheme) {
            breaks.push(grapheme_index);
        }
    }
    if breaks.is_empty() {
        None
    } else {
        Some(breaks)
    }
}

fn add_internal_letter_spacing(width: f32, grapheme_count: usize, letter_spacing: f32) -> f32 {
    if grapheme_count > 1 {
        width + (grapheme_count as f32 - 1.0) * letter_spacing
    } else {
        width
    }
}

// ===== measure_analysis — the prepare-time builder =====

#[allow(clippy::too_many_arguments)]
fn measure_analysis<B: MeasureBackend>(
    analysis: &TextAnalysis,
    caches: &mut MeasurementCaches,
    backend: &B,
    font: &str,
    include_segments: bool,
    word_break: WordBreakMode,
    letter_spacing: f32,
) -> PreparedTextWithSegments {
    let engine_profile = get_engine_profile();
    let mut state: FontMeasurementState<'_, B> = get_font_measurement_state(
        caches,
        backend,
        font,
        text_may_contain_emoji(&analysis.normalized),
    );
    let discretionary_hyphen_width = {
        let mut m = state.get_segment_metrics("-");
        get_corrected_segment_width("-", &mut m, state.emoji_correction)
    } + if letter_spacing == 0.0 { 0.0 } else { letter_spacing * 2.0 };
    let space_width = {
        let mut m = state.get_segment_metrics(" ");
        get_corrected_segment_width(" ", &mut m, state.emoji_correction)
    };
    let tab_stop_advance = space_width * 8.0;
    let has_letter_spacing = letter_spacing != 0.0;

    if analysis.len == 0 {
        state.commit(caches);
        return create_empty_prepared(include_segments);
    }

    let mut widths: Vec<f32> = vec![];
    let mut line_end_fit_advances: Vec<f32> = vec![];
    let mut line_end_paint_advances: Vec<f32> = vec![];
    let mut kinds: Vec<SegmentBreakKind> = vec![];
    let mut simple_line_walk_fast_path =
        analysis.chunks.len() <= 1 && !has_letter_spacing;
    let mut seg_starts: Vec<usize> = vec![];
    let mut breakable_fit_advances: Vec<Option<Vec<f32>>> = vec![];
    let mut breakable_preferred_breaks: Vec<Option<Vec<usize>>> = vec![];
    let mut spacing_grapheme_counts: Vec<usize> = vec![];
    let mut segments: Vec<String> = vec![];
    let mut prepared_start_by_analysis_index: Vec<usize> = vec![0; analysis.len];

    let push_measured_segment = |widths: &mut Vec<f32>,
                                 line_end_fit_advances: &mut Vec<f32>,
                                 line_end_paint_advances: &mut Vec<f32>,
                                 kinds: &mut Vec<SegmentBreakKind>,
                                 seg_starts: &mut Vec<usize>,
                                 breakable_fit_advances: &mut Vec<Option<Vec<f32>>>,
                                 breakable_preferred_breaks: &mut Vec<Option<Vec<usize>>>,
                                 spacing_grapheme_counts: &mut Vec<usize>,
                                 segments: &mut Vec<String>,
                                 simple_fast_path: &mut bool,
                                 has_letter_spacing: bool,
                                 text: String,
                                 width: f32,
                                 line_end_fit_advance: f32,
                                 line_end_paint_advance: f32,
                                 kind: SegmentBreakKind,
                                 start: usize,
                                 breakable_fit_advance: Option<Vec<f32>>,
                                 breakable_preferred_break: Option<Vec<usize>>,
                                 spacing_grapheme_count: usize| {
        if !matches!(kind, SegmentBreakKind::Text | SegmentBreakKind::Space | SegmentBreakKind::ZeroWidthBreak) {
            *simple_fast_path = false;
        }
        widths.push(width);
        line_end_fit_advances.push(line_end_fit_advance);
        line_end_paint_advances.push(line_end_paint_advance);
        kinds.push(kind);
        seg_starts.push(start);
        breakable_fit_advances.push(breakable_fit_advance);
        breakable_preferred_breaks.push(breakable_preferred_break);
        if has_letter_spacing {
            spacing_grapheme_counts.push(spacing_grapheme_count);
        }
        if !text.is_empty() {
            // TS pushes the segment text regardless of includeSegments flag
            // when `segments !== null`; we always push since they are cheap,
            // and `include_segments = false` callers (PreparedText) drop them
            // by calling into a different builder.
        }
        segments.push(text);
    };

    let mut push_measured_text_segment = |state: &mut FontMeasurementState<'_, B>,
                                          push: &mut dyn FnMut(
        String,
        f32,
        f32,
        f32,
        SegmentBreakKind,
        usize,
        Option<Vec<f32>>,
        Option<Vec<usize>>,
        usize,
    ),
                                          text: String,
                                          kind: SegmentBreakKind,
                                          start: usize,
                                          word_like: bool,
                                          allow_overflow_breaks: bool,
                                          engine_profile: EngineProfile,
                                          letter_spacing: f32,
                                          word_break: WordBreakMode| {
        let text_metrics = state.get_segment_metrics(&text);
        let spacing_grapheme_count =
            if has_letter_spacing { count_rendered_spacing_graphemes(&text, kind) } else { 0 };
        let width = add_internal_letter_spacing(
            get_corrected_segment_width(&text, &mut text_metrics.clone(), state.emoji_correction),
            spacing_grapheme_count,
            letter_spacing,
        );
        let base_line_end_fit_advance = if matches!(
            kind,
            SegmentBreakKind::Space | SegmentBreakKind::PreservedSpace | SegmentBreakKind::ZeroWidthBreak
        ) {
            0.0
        } else {
            width
        };
        let line_end_fit_advance = if base_line_end_fit_advance == 0.0 {
            0.0
        } else {
            base_line_end_fit_advance + if spacing_grapheme_count > 0 { letter_spacing } else { 0.0 }
        };
        let line_end_paint_advance =
            if matches!(kind, SegmentBreakKind::Space | SegmentBreakKind::ZeroWidthBreak) {
                0.0
            } else {
                width
            };

        if allow_overflow_breaks && word_like && text.chars().count() > 1 {
            let fit_mode = if letter_spacing != 0.0 {
                BreakableFitMode::SegmentPrefixes
            } else if is_numeric_run_segment(&text) {
                BreakableFitMode::PairContext
            } else if engine_profile.prefer_prefix_widths_for_breakable_runs {
                BreakableFitMode::SegmentPrefixes
            } else {
                BreakableFitMode::SumGraphemes
            };
            let fit_advances = state.get_segment_breakable_fit_advances(&text, &mut text_metrics.clone(), fit_mode);
            let preferred_breaks = if fit_advances.is_none() || word_break == WordBreakMode::KeepAll {
                None
            } else {
                get_breakable_preferred_breaks(&text)
            };
            push(
                text,
                width,
                line_end_fit_advance,
                line_end_paint_advance,
                kind,
                start,
                fit_advances,
                preferred_breaks,
                spacing_grapheme_count,
            );
            return;
        }
        push(
            text,
            width,
            line_end_fit_advance,
            line_end_paint_advance,
            kind,
            start,
            None,
            None,
            spacing_grapheme_count,
        );
    };

    for mi in 0..analysis.len {
        prepared_start_by_analysis_index[mi] = widths.len();
        let seg_text = analysis.texts[mi].clone();
        let seg_word_like = analysis.is_word_like[mi];
        let seg_kind = analysis.kinds[mi];
        let seg_start = analysis.starts[mi];

        match seg_kind {
            SegmentBreakKind::SoftHyphen => {
                push_measured_segment(
                    &mut widths,
                    &mut line_end_fit_advances,
                    &mut line_end_paint_advances,
                    &mut kinds,
                    &mut seg_starts,
                    &mut breakable_fit_advances,
                    &mut breakable_preferred_breaks,
                    &mut spacing_grapheme_counts,
                    &mut segments,
                    &mut simple_line_walk_fast_path,
                    has_letter_spacing,
                    seg_text,
                    0.0,
                    discretionary_hyphen_width,
                    discretionary_hyphen_width,
                    seg_kind,
                    seg_start,
                    None,
                    None,
                    0,
                );
                continue;
            }
            SegmentBreakKind::HardBreak => {
                push_measured_segment(
                    &mut widths,
                    &mut line_end_fit_advances,
                    &mut line_end_paint_advances,
                    &mut kinds,
                    &mut seg_starts,
                    &mut breakable_fit_advances,
                    &mut breakable_preferred_breaks,
                    &mut spacing_grapheme_counts,
                    &mut segments,
                    &mut simple_line_walk_fast_path,
                    has_letter_spacing,
                    seg_text,
                    0.0,
                    0.0,
                    0.0,
                    seg_kind,
                    seg_start,
                    None,
                    None,
                    0,
                );
                continue;
            }
            SegmentBreakKind::Tab => {
                push_measured_segment(
                    &mut widths,
                    &mut line_end_fit_advances,
                    &mut line_end_paint_advances,
                    &mut kinds,
                    &mut seg_starts,
                    &mut breakable_fit_advances,
                    &mut breakable_preferred_breaks,
                    &mut spacing_grapheme_counts,
                    &mut segments,
                    &mut simple_line_walk_fast_path,
                    has_letter_spacing,
                    seg_text.clone(),
                    0.0,
                    0.0,
                    0.0,
                    seg_kind,
                    seg_start,
                    None,
                    None,
                    if has_letter_spacing {
                        count_rendered_spacing_graphemes(&seg_text, seg_kind)
                    } else {
                        0
                    },
                );
                continue;
            }
            _ => {}
        }

        let seg_metrics = state.get_segment_metrics(&seg_text);

        if seg_kind == SegmentBreakKind::Text && seg_metrics.contains_cjk {
            let base_units = build_base_cjk_units(&seg_text, engine_profile);
            let measured_units = if word_break == WordBreakMode::KeepAll {
                merge_keep_all_text_units(
                    &seg_text,
                    base_units,
                    engine_profile.break_keep_all_after_punctuation,
                )
            } else {
                base_units
            };
            for unit in measured_units.iter() {
                let unit_text = unit.text.clone();
                let unit_start = seg_start + unit.start;
                let allow_overflow_breaks =
                    word_break == WordBreakMode::KeepAll || !is_cjk(&unit_text);
                push_measured_text_segment(
                    &mut state,
                    &mut |text, width, lefa, lepa, kind, start, bfa, bpb, sgc| {
                        push_measured_segment(
                            &mut widths,
                            &mut line_end_fit_advances,
                            &mut line_end_paint_advances,
                            &mut kinds,
                            &mut seg_starts,
                            &mut breakable_fit_advances,
                            &mut breakable_preferred_breaks,
                            &mut spacing_grapheme_counts,
                            &mut segments,
                            &mut simple_line_walk_fast_path,
                            has_letter_spacing,
                            text,
                            width,
                            lefa,
                            lepa,
                            kind,
                            start,
                            bfa,
                            bpb,
                            sgc,
                        );
                    },
                    unit_text,
                    SegmentBreakKind::Text,
                    unit_start,
                    seg_word_like,
                    allow_overflow_breaks,
                    engine_profile,
                    letter_spacing,
                    word_break,
                );
            }
            continue;
        }

        push_measured_text_segment(
            &mut state,
            &mut |text, width, lefa, lepa, kind, start, bfa, bpb, sgc| {
                push_measured_segment(
                    &mut widths,
                    &mut line_end_fit_advances,
                    &mut line_end_paint_advances,
                    &mut kinds,
                    &mut seg_starts,
                    &mut breakable_fit_advances,
                    &mut breakable_preferred_breaks,
                    &mut spacing_grapheme_counts,
                    &mut segments,
                    &mut simple_line_walk_fast_path,
                    has_letter_spacing,
                    text,
                    width,
                    lefa,
                    lepa,
                    kind,
                    start,
                    bfa,
                    bpb,
                    sgc,
                );
            },
            seg_text,
            seg_kind,
            seg_start,
            seg_word_like,
            true,
            engine_profile,
            letter_spacing,
            word_break,
        );
    }

    let chunks = map_analysis_chunks_to_prepared_chunks(
        &analysis.chunks,
        &prepared_start_by_analysis_index,
        widths.len(),
    );
    let seg_levels = compute_segment_levels(&analysis.normalized, &seg_starts);
    state.commit(caches);

    PreparedTextWithSegments {
        prepared: PreparedLineBreakData {
            widths,
            line_end_fit_advances,
            line_end_paint_advances,
            kinds,
            simple_line_walk_fast_path,
            breakable_fit_advances,
            breakable_preferred_breaks,
            letter_spacing,
            spacing_grapheme_counts,
            discretionary_hyphen_width,
            tab_stop_advance,
            chunks,
        },
        seg_levels,
        segments: if include_segments { segments } else { vec![] },
    }
}

fn map_analysis_chunks_to_prepared_chunks(
    chunks: &[AnalysisChunk],
    prepared_start_by_analysis_index: &[usize],
    prepared_end_segment_index: usize,
) -> Vec<crate::lyricstyles::sonnet_v2::pretext::line_break::PreparedChunk> {
    use crate::lyricstyles::sonnet_v2::pretext::line_break::PreparedChunk;
    let mut prepared_chunks: Vec<PreparedChunk> = vec![];
    for chunk in chunks.iter() {
        let start_segment_index = if chunk.start_segment_index < prepared_start_by_analysis_index.len() {
            prepared_start_by_analysis_index[chunk.start_segment_index]
        } else {
            prepared_end_segment_index
        };
        let end_segment_index = if chunk.end_segment_index < prepared_start_by_analysis_index.len() {
            prepared_start_by_analysis_index[chunk.end_segment_index]
        } else {
            prepared_end_segment_index
        };
        let consumed_end_segment_index =
            if chunk.consumed_end_segment_index < prepared_start_by_analysis_index.len() {
                prepared_start_by_analysis_index[chunk.consumed_end_segment_index]
            } else {
                prepared_end_segment_index
            };
        prepared_chunks.push(PreparedChunk {
            start_segment_index,
            end_segment_index,
            consumed_end_segment_index,
        });
    }
    prepared_chunks
}

// ===== prepareInternal + public prepare entry points =====

pub fn prepare_internal<B: MeasureBackend>(
    text: &str,
    caches: &mut MeasurementCaches,
    backend: &B,
    font: &str,
    include_segments: bool,
    options: PrepareOptions,
) -> PreparedTextWithSegments {
    // TS: wordBreak = options?.wordBreak ?? 'normal'
    // TS: letterSpacing = options?.letterSpacing ?? 0
    let ep = get_engine_profile();
    let analysis_profile = AnalysisProfile {
        carry_cjk_after_closing_quote: ep.carry_cjk_after_closing_quote,
        break_keep_all_after_punctuation: ep.break_keep_all_after_punctuation,
    };
    let analysis = analyze_text(text, analysis_profile, options.white_space, options.word_break);
    measure_analysis(
        &analysis,
        caches,
        backend,
        font,
        include_segments,
        options.word_break,
        options.letter_spacing,
    )
}

pub fn prepare<B: MeasureBackend>(
    text: &str,
    caches: &mut MeasurementCaches,
    backend: &B,
    font: &str,
    options: PrepareOptions,
) -> PreparedText {
    // TS: prepareInternal(text, font, false, options)
    let rich = prepare_internal(text, caches, backend, font, false, options);
    PreparedText {
        prepared: rich.prepared,
        seg_levels: rich.seg_levels,
    }
}

// ===== create_layout_line / create_layout_line_range =====

fn create_layout_line(
    prepared: &PreparedTextWithSegments,
    cache: &LineTextCache,
    width: f32,
    start_segment_index: usize,
    start_grapheme_index: usize,
    end_segment_index: usize,
    end_grapheme_index: usize,
) -> LayoutLine {
    LayoutLine {
        text: build_line_text_from_range(
            &prepared.as_view(),
            cache,
            start_segment_index,
            start_grapheme_index,
            end_segment_index,
            end_grapheme_index,
        ),
        width,
        start: LayoutCursor {
            segment_index: start_segment_index,
            grapheme_index: start_grapheme_index,
        },
        end: LayoutCursor {
            segment_index: end_segment_index,
            grapheme_index: end_grapheme_index,
        },
    }
}

fn create_layout_line_range(
    width: f32,
    start_segment_index: usize,
    start_grapheme_index: usize,
    end_segment_index: usize,
    end_grapheme_index: usize,
) -> LayoutLineRange {
    LayoutLineRange {
        width,
        start: LayoutCursor {
            segment_index: start_segment_index,
            grapheme_index: start_grapheme_index,
        },
        end: LayoutCursor {
            segment_index: end_segment_index,
            grapheme_index: end_grapheme_index,
        },
    }
}

// ===== public layout APIs =====

pub fn layout(prepared: &PreparedText, max_width: f32, line_height: f32) -> LayoutResult {
    // Keep the resize hot path specialized. `layoutWithLines()` shares the same
    // break semantics but also tracks line ranges; the extra bookkeeping is too
    // expensive to pay on every hot-path `layout()` call.
    let line_count = count_prepared_lines(&prepared.prepared, max_width);
    LayoutResult { line_count, height: (line_count as f32) * line_height }
}

pub fn materialize_line_range(
    prepared: &PreparedTextWithSegments,
    line: LayoutLineRange,
) -> LayoutLine {
    let cache = get_line_text_cache();
    create_layout_line(
        prepared,
        &cache,
        line.width,
        line.start.segment_index,
        line.start.grapheme_index,
        line.end.segment_index,
        line.end.grapheme_index,
    )
}

pub fn walk_line_ranges(
    prepared: &PreparedTextWithSegments,
    max_width: f32,
    mut on_line: impl FnMut(LayoutLineRange),
) -> usize {
    if prepared.prepared.widths.is_empty() {
        return 0;
    }
    let mut visitor = move |width: f32, ssi: usize, sgi: usize, esi: usize, egi: usize| {
        on_line(create_layout_line_range(width, ssi, sgi, esi, egi));
    };
    walk_prepared_lines_raw(&prepared.prepared, max_width, Some(&mut visitor))
}

pub fn measure_line_stats(prepared: &PreparedTextWithSegments, max_width: f32) -> LineStats {
    let LineGeometryStats { line_count, max_line_width } =
        measure_prepared_line_geometry(&prepared.prepared, max_width);
    LineStats { line_count, max_line_width }
}

// Intrinsic-width helper for rich/userland layout work. This asks "how wide is
// the prepared text when container width is not the thing forcing wraps?".
// Explicit hard breaks still count, so this returns the widest forced line.
pub fn measure_natural_width(prepared: &PreparedTextWithSegments) -> f32 {
    let mut max_width: f32 = 0.0;
    let mut visitor = |width: f32, _: usize, _: usize, _: usize, _: usize| {
        if width > max_width {
            max_width = width;
        }
    };
    walk_prepared_lines_raw(&prepared.prepared, f32::INFINITY, Some(&mut visitor));
    max_width
}

pub fn layout_next_line(
    prepared: &PreparedTextWithSegments,
    start: LayoutCursor,
    max_width: f32,
) -> Option<LayoutLine> {
    let mut end = LbCursor {
        segment_index: start.segment_index,
        grapheme_index: start.grapheme_index,
    };
    let chunk_index = normalize_prepared_line_start(&prepared.prepared, &mut end);
    if chunk_index < 0 {
        return None;
    }
    let line_start_segment_index = end.segment_index;
    let line_start_grapheme_index = end.grapheme_index;
    let width = step_prepared_line_geometry_from_chunk(&prepared.prepared, &mut end, chunk_index as usize, max_width)?;
    Some(create_layout_line(
        prepared,
        &get_line_text_cache(),
        width,
        line_start_segment_index,
        line_start_grapheme_index,
        end.segment_index,
        end.grapheme_index,
    ))
}

pub fn layout_next_line_range(
    prepared: &PreparedTextWithSegments,
    start: LayoutCursor,
    max_width: f32,
) -> Option<LayoutLineRange> {
    let mut end = LbCursor {
        segment_index: start.segment_index,
        grapheme_index: start.grapheme_index,
    };
    let chunk_index = normalize_prepared_line_start(&prepared.prepared, &mut end);
    if chunk_index < 0 {
        return None;
    }
    let line_start_segment_index = end.segment_index;
    let line_start_grapheme_index = end.grapheme_index;
    let width = step_prepared_line_geometry_from_chunk(&prepared.prepared, &mut end, chunk_index as usize, max_width)?;
    Some(create_layout_line_range(
        width,
        line_start_segment_index,
        line_start_grapheme_index,
        end.segment_index,
        end.grapheme_index,
    ))
}

pub fn layout_with_lines(
    prepared: &PreparedTextWithSegments,
    max_width: f32,
    line_height: f32,
) -> LayoutLinesResult {
    let mut lines: Vec<LayoutLine> = vec![];
    if prepared.prepared.widths.is_empty() {
        return LayoutLinesResult { line_count: 0, height: 0.0, lines };
    }
    let grapheme_cache = get_line_text_cache();
    let mut visitor = |width: f32, ssi: usize, sgi: usize, esi: usize, egi: usize| {
        lines.push(create_layout_line(
            prepared,
            &grapheme_cache,
            width,
            ssi,
            sgi,
            esi,
            egi,
        ));
    };
    let line_count = walk_prepared_lines_raw(&prepared.prepared, max_width, Some(&mut visitor));
    LayoutLinesResult {
        line_count,
        height: (line_count as f32) * line_height,
        lines,
    }
}

pub fn clear_cache() {
    // clearAnalysisCaches() — no global caches in Rust (caches belong to the
    // caller-owned `MeasurementCaches`), so this is a no-op. The line-text
    // caches map inside `PreparedTextWithSegments` are reclaimed when those
    // structs drop.
    let mut cache = get_line_text_cache();
    clear_line_text_caches(&mut cache);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::pretext::measurement::{MeasurementCaches, MeasureBackend, SegmentMetrics};

    struct ByteLenBackend;
    impl MeasureBackend for ByteLenBackend {
        fn measure_text(&self, text: &str, _font_str: &str) -> f32 {
            text.chars().count() as f32
        }
    }

    #[test]
    fn prepare_latin_text_and_layout_wraps_by_width() {
        // Each char has advance 1.0; at 5.0 max width, "hello world" (11 chars)
        // must fold to at least 2 lines (no preferred break points → overflow).
        let backend = ByteLenBackend;
        let mut caches = MeasurementCaches::default();
        let prepared_with_segs = prepare_internal(
            "hello world",
            &mut caches,
            &backend,
            "500 24px test",
            true,
            PrepareOptions::default(),
        );
        assert!(prepared_with_segs.prepared.widths.len() >= 2,
            "expected >=2 segment widths after prepare, got {}",
            prepared_with_segs.prepared.widths.len());

        let stats = measure_line_stats(&prepared_with_segs, 5.0);
        assert!(stats.line_count >= 2,
            "expected line_count >= 2 for 11-char text at width=5, got {}",
            stats.line_count);

        // layout (non-materializing) should match.
        let prepared = PreparedText {
            prepared: prepared_with_segs.prepared.clone(),
            seg_levels: prepared_with_segs.seg_levels.clone(),
        };
        let layout_result = layout(&prepared, 5.0, 20.0);
        assert_eq!(layout_result.line_count, stats.line_count);
        assert_eq!(layout_result.height, (stats.line_count as f32) * 20.0);
    }

    #[test]
    fn layout_with_lines_materializes_each_line() {
        let backend = ByteLenBackend;
        let mut caches = MeasurementCaches::default();
        let prepared = prepare_internal(
            "abc def",
            &mut caches,
            &backend,
            "500 16px test",
            true,
            PrepareOptions::default(),
        );
        let res = layout_with_lines(&prepared, 3.0, 16.0);
        // "abc def" folds at the word boundary. Pretext's buildLineTextFromRange
        // preserves the trailing space at the break boundary in the first line's
        // text — this is byte-faithful to the TS source.
        assert_eq!(res.line_count, 2, "expected 2 lines for 'abc def' at width 3");
        assert_eq!(res.lines.len(), 2);
        assert_eq!(res.height, 2.0 * 16.0);
        assert_eq!(res.lines[0].text, "abc ");
        assert_eq!(res.lines[1].text, "def");
    }
}
