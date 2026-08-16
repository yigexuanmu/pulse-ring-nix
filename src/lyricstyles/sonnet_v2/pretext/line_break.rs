//! Folia sonnet v2 — pretext `line-break.ts` (1236 lines) `compiler-grade 1:1 port`.
//!
//! Pure fold algorithm over a `PreparedLineBreakData` snapshot. The TS source
//! uses deeply nested closures that capture mutable outer state; in Rust this
//! is reproduced with state-struct methods (`&mut self`) so each shared-mutate
//! closure maps 1:1 to a method call.
//!
//! See `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` Phase 2.3.

use crate::lyricstyles::sonnet_v2::pretext::analysis::SegmentBreakKind;
use crate::lyricstyles::sonnet_v2::pretext::measurement::get_engine_profile;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineBreakCursor {
    pub segment_index: usize,
    pub grapheme_index: usize,
}

#[derive(Clone, Debug)]
pub struct PreparedChunk {
    pub start_segment_index: usize,
    pub end_segment_index: usize,
    pub consumed_end_segment_index: usize,
}

/// Faithful port of `line-break.ts`'s `PreparedLineBreakData` interface — an
/// owned snapshot layout.rs builds from its `PreparedTextWithSegments`.
#[derive(Clone, Debug)]
pub struct PreparedLineBreakData {
    pub widths: Vec<f32>,
    pub line_end_fit_advances: Vec<f32>,
    pub line_end_paint_advances: Vec<f32>,
    pub kinds: Vec<SegmentBreakKind>,
    pub simple_line_walk_fast_path: bool,
    pub breakable_fit_advances: Vec<Option<Vec<f32>>>,
    pub breakable_preferred_breaks: Vec<Option<Vec<usize>>>,
    pub letter_spacing: f32,
    pub spacing_grapheme_counts: Vec<usize>,
    pub discretionary_hyphen_width: f32,
    pub tab_stop_advance: f32,
    pub chunks: Vec<PreparedChunk>,
}

pub type LineVisitor<'a> = &'a mut dyn FnMut(f32, usize, usize, usize, usize);

// ===== free helpers (analysis-independent) =====

fn consumes_at_line_start(kind: SegmentBreakKind) -> bool {
    matches!(
        kind,
        SegmentBreakKind::Space | SegmentBreakKind::ZeroWidthBreak | SegmentBreakKind::SoftHyphen
    )
}

fn breaks_after(kind: SegmentBreakKind) -> bool {
    matches!(
        kind,
        SegmentBreakKind::Space
            | SegmentBreakKind::PreservedSpace
            | SegmentBreakKind::Tab
            | SegmentBreakKind::ZeroWidthBreak
            | SegmentBreakKind::SoftHyphen
    )
}

fn normalize_line_start_segment_index(
    prepared: &PreparedLineBreakData,
    mut segment_index: usize,
    end_segment_index: usize,
) -> usize {
    while segment_index < end_segment_index {
        let kind = prepared.kinds[segment_index];
        if !consumes_at_line_start(kind) {
            break;
        }
        segment_index += 1;
    }
    segment_index
}

fn get_tab_advance(line_width: f32, tab_stop_advance: f32) -> f32 {
    if tab_stop_advance <= 0.0 {
        return 0.0;
    }
    let remainder = line_width % tab_stop_advance;
    if remainder.abs() <= 1e-6 {
        tab_stop_advance
    } else {
        tab_stop_advance - remainder
    }
}

fn get_leading_letter_spacing(
    prepared: &PreparedLineBreakData,
    has_content: bool,
    segment_index: usize,
) -> f32 {
    if prepared.letter_spacing != 0.0
        && has_content
        && prepared.spacing_grapheme_counts[segment_index] > 0
    {
        prepared.letter_spacing
    } else {
        0.0
    }
}

fn get_line_end_contribution(leading_spacing: f32, segment_contribution: f32) -> f32 {
    if segment_contribution == 0.0 {
        0.0
    } else {
        leading_spacing + segment_contribution
    }
}

fn get_tab_trailing_letter_spacing(prepared: &PreparedLineBreakData, segment_index: usize) -> f32 {
    if prepared.letter_spacing != 0.0 && prepared.spacing_grapheme_counts[segment_index] > 0 {
        prepared.letter_spacing
    } else {
        0.0
    }
}

fn get_whole_segment_fit_contribution(
    prepared: &PreparedLineBreakData,
    kind: SegmentBreakKind,
    segment_index: usize,
    leading_spacing: f32,
    segment_width: f32,
) -> f32 {
    let segment_contribution = match kind {
        SegmentBreakKind::Tab => segment_width + get_tab_trailing_letter_spacing(prepared, segment_index),
        _ => prepared.line_end_fit_advances[segment_index],
    };
    get_line_end_contribution(leading_spacing, segment_contribution)
}

fn get_break_opportunity_fit_contribution(
    prepared: &PreparedLineBreakData,
    kind: SegmentBreakKind,
    segment_index: usize,
    leading_spacing: f32,
) -> f32 {
    let segment_contribution = match kind {
        SegmentBreakKind::Tab => 0.0,
        _ => prepared.line_end_fit_advances[segment_index],
    };
    get_line_end_contribution(leading_spacing, segment_contribution)
}

fn get_line_end_paint_contribution(
    prepared: &PreparedLineBreakData,
    kind: SegmentBreakKind,
    segment_index: usize,
    leading_spacing: f32,
    segment_width: f32,
) -> f32 {
    let segment_contribution = match kind {
        SegmentBreakKind::Tab => segment_width,
        _ => prepared.line_end_paint_advances[segment_index],
    };
    get_line_end_contribution(leading_spacing, segment_contribution)
}

fn get_breakable_grapheme_advance(
    prepared: &PreparedLineBreakData,
    has_content: bool,
    base_advance: f32,
) -> f32 {
    if prepared.letter_spacing != 0.0 && has_content {
        base_advance + prepared.letter_spacing
    } else {
        base_advance
    }
}

fn get_breakable_candidate_fit_width(
    prepared: &PreparedLineBreakData,
    candidate_paint_width: f32,
) -> f32 {
    if prepared.letter_spacing == 0.0 {
        candidate_paint_width
    } else {
        candidate_paint_width + prepared.letter_spacing
    }
}

fn get_next_preferred_break_index(preferred_breaks: &[usize], mut index: usize, grapheme_end: usize) -> usize {
    while index < preferred_breaks.len() && preferred_breaks[index] < grapheme_end {
        index += 1;
    }
    index
}

fn get_terminal_letter_spacing(
    prepared: &PreparedLineBreakData,
    start_segment_index: usize,
    start_grapheme_index: usize,
    end_segment_index: usize,
    end_grapheme_index: usize,
) -> f32 {
    if prepared.letter_spacing == 0.0 {
        return 0.0;
    }

    if end_grapheme_index > 0 {
        return if prepared.spacing_grapheme_counts[end_segment_index] > 0 {
            prepared.letter_spacing
        } else {
            0.0
        };
    }

    let mut i = end_segment_index as i64 - 1;
    while i >= start_segment_index as i64 {
        let idx = i as usize;
        let kind = prepared.kinds[idx];
        if matches!(
            kind,
            SegmentBreakKind::Space | SegmentBreakKind::ZeroWidthBreak | SegmentBreakKind::HardBreak
        ) {
            i -= 1;
            continue;
        }
        if matches!(kind, SegmentBreakKind::SoftHyphen) {
            if i as usize == end_segment_index - 1 {
                return 0.0;
            }
            i -= 1;
            continue;
        }

        if idx == start_segment_index && start_grapheme_index > 0 {
            return prepared.letter_spacing;
        }

        return if prepared.spacing_grapheme_counts[idx] > 0 {
            prepared.letter_spacing
        } else {
            0.0
        };
    }

    0.0
}

fn finalize_line_paint_width(
    prepared: &PreparedLineBreakData,
    width: f32,
    start_segment_index: usize,
    start_grapheme_index: usize,
    end_segment_index: usize,
    end_grapheme_index: usize,
) -> f32 {
    width + get_terminal_letter_spacing(
        prepared,
        start_segment_index,
        start_grapheme_index,
        end_segment_index,
        end_grapheme_index,
    )
}

fn find_chunk_index_for_start(prepared: &PreparedLineBreakData, segment_index: usize) -> i64 {
    let mut lo = 0usize;
    let mut hi = prepared.chunks.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if segment_index < prepared.chunks[mid].consumed_end_segment_index {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    if lo < prepared.chunks.len() {
        lo as i64
    } else {
        -1
    }
}

fn normalize_line_start_in_chunk(
    prepared: &PreparedLineBreakData,
    chunk_index: usize,
    cursor: &mut LineBreakCursor,
) -> i64 {
    let mut segment_index = cursor.segment_index;
    if cursor.grapheme_index > 0 {
        return chunk_index as i64;
    }
    let chunk = &prepared.chunks[chunk_index];
    if chunk.start_segment_index == chunk.end_segment_index
        && segment_index == chunk.start_segment_index
    {
        cursor.segment_index = segment_index;
        cursor.grapheme_index = 0;
        return chunk_index as i64;
    }
    if segment_index < chunk.start_segment_index {
        segment_index = chunk.start_segment_index;
    }
    segment_index = normalize_line_start_segment_index(prepared, segment_index, chunk.end_segment_index);
    if segment_index < chunk.end_segment_index {
        cursor.segment_index = segment_index;
        cursor.grapheme_index = 0;
        return chunk_index as i64;
    }
    if chunk.consumed_end_segment_index >= prepared.widths.len() {
        return -1;
    }
    cursor.segment_index = chunk.consumed_end_segment_index;
    cursor.grapheme_index = 0;
    (chunk_index + 1) as i64
}

pub fn normalize_prepared_line_start(
    prepared: &PreparedLineBreakData,
    cursor: &mut LineBreakCursor,
) -> i64 {
    if cursor.segment_index >= prepared.widths.len() {
        return -1;
    }
    let chunk_index = find_chunk_index_for_start(prepared, cursor.segment_index);
    if chunk_index < 0 {
        return -1;
    }
    normalize_line_start_in_chunk(prepared, chunk_index as usize, cursor)
}

fn normalize_line_start_chunk_index_from_hint(
    prepared: &PreparedLineBreakData,
    chunk_index: usize,
    cursor: &mut LineBreakCursor,
) -> i64 {
    if cursor.segment_index >= prepared.widths.len() {
        return -1;
    }
    let mut next_chunk_index = chunk_index;
    while next_chunk_index < prepared.chunks.len()
        && cursor.segment_index >= prepared.chunks[next_chunk_index].consumed_end_segment_index
    {
        next_chunk_index += 1;
    }
    if next_chunk_index >= prepared.chunks.len() {
        return -1;
    }
    normalize_line_start_in_chunk(prepared, next_chunk_index, cursor)
}

pub fn count_prepared_lines(prepared: &PreparedLineBreakData, max_width: f32) -> usize {
    walk_prepared_lines_raw(prepared, max_width, None)
}

// ===== SimpleWalker (maps walkPreparedLinesSimple's outer state) =====

struct SimpleWalker<'a> {
    prepared: &'a PreparedLineBreakData,
    fit_limit: f32,
    line_count: usize,
    line_w: f32,
    has_content: bool,
    line_start_segment_index: usize,
    line_start_grapheme_index: usize,
    line_end_segment_index: usize,
    line_end_grapheme_index: usize,
    pending_break_segment_index: i64,
    pending_break_paint_width: f32,
}

impl<'a> SimpleWalker<'a> {
    fn clear_pending_break(&mut self) {
        self.pending_break_segment_index = -1;
        self.pending_break_paint_width = 0.0;
    }

    fn emit_current_line_default(&mut self, on_line: &mut Option<LineVisitor<'_>>) {
        let es = self.line_end_segment_index;
        let eg = self.line_end_grapheme_index;
        let lw = self.line_w;
        self.emit_current_line(es, eg, lw, on_line);
    }

    fn emit_current_line(
        &mut self,
        end_segment_index: usize,
        end_grapheme_index: usize,
        width: f32,
        on_line: &mut Option<LineVisitor<'_>>,
    ) {
        self.line_count += 1;
        if let Some(cb) = on_line.as_deref_mut() {
            cb(
                width,
                self.line_start_segment_index,
                self.line_start_grapheme_index,
                end_segment_index,
                end_grapheme_index,
            );
        }
        self.line_w = 0.0;
        self.has_content = false;
        self.clear_pending_break();
    }

    fn start_line_at_segment(&mut self, segment_index: usize, width: f32) {
        self.has_content = true;
        self.line_start_segment_index = segment_index;
        self.line_start_grapheme_index = 0;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
        self.line_w = width;
    }

    fn start_line_at_grapheme(
        &mut self,
        segment_index: usize,
        grapheme_index: usize,
        width: f32,
    ) {
        self.has_content = true;
        self.line_start_segment_index = segment_index;
        self.line_start_grapheme_index = grapheme_index;
        self.line_end_segment_index = segment_index;
        self.line_end_grapheme_index = grapheme_index + 1;
        self.line_w = width;
    }

    fn append_whole_segment(&mut self, segment_index: usize, width: f32) {
        if !self.has_content {
            self.start_line_at_segment(segment_index, width);
            return;
        }
        self.line_w += width;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
    }

    fn append_breakable_segment_from(
        &mut self,
        segment_index: usize,
        start_grapheme_index: usize,
        on_line: &mut Option<LineVisitor<'_>>,
    ) {
        let fit_advances = self.prepared.breakable_fit_advances[segment_index]
            .as_ref()
            .expect("breakable segment must carry fit advances");
        let preferred_breaks = self.prepared.breakable_preferred_breaks[segment_index]
            .as_ref();
        let mut preferred_break_index = match preferred_breaks {
            Some(pb) => get_next_preferred_break_index(pb, 0, start_grapheme_index + 1),
            None => usize::MAX,
        };
        let mut last_preferred_break_end: i64 = -1;
        let mut last_preferred_break_width = 0.0;

        let mut g = start_grapheme_index;
        while g < fit_advances.len() {
            let gw = fit_advances[g];

            if !self.has_content {
                self.start_line_at_grapheme(segment_index, g, gw);
            } else if self.line_w + gw > self.fit_limit {
                if let Some(pb) = preferred_breaks {
                    if last_preferred_break_end > start_grapheme_index as i64 {
                        let lpe = last_preferred_break_end as usize;
                        let lpw = last_preferred_break_width;
                        self.line_w = lpw;
                        self.emit_current_line(segment_index, lpe, lpw, on_line);
                        g = lpe;
                        preferred_break_index = get_next_preferred_break_index(pb, preferred_break_index, g + 1);
                        last_preferred_break_end = -1;
                        last_preferred_break_width = 0.0;
                        continue;
                    }
                }
                self.emit_current_line_default(on_line);
                self.start_line_at_grapheme(segment_index, g, gw);
            } else {
                self.line_w += gw;
                self.line_end_segment_index = segment_index;
                self.line_end_grapheme_index = g + 1;
            }

            let grapheme_end = g + 1;
            if let Some(pb) = preferred_breaks {
                if preferred_break_index < pb.len() && pb[preferred_break_index] == grapheme_end {
                    last_preferred_break_end = grapheme_end as i64;
                    last_preferred_break_width = self.line_w;
                    preferred_break_index += 1;
                }
            }
            g += 1;
        }

        if self.has_content
            && self.line_end_segment_index == segment_index
            && self.line_end_grapheme_index == fit_advances.len()
        {
            self.line_end_segment_index = segment_index + 1;
            self.line_end_grapheme_index = 0;
        }
    }

    fn walk(mut self, max_width: f32, mut on_line: Option<LineVisitor<'_>>) -> usize {
        if self.prepared.widths.is_empty() {
            return 0;
        }
        self.fit_limit = max_width + get_engine_profile().line_fit_epsilon;

        let mut i = 0usize;
        while i < self.prepared.widths.len() {
            if !self.has_content {
                i = normalize_line_start_segment_index(self.prepared, i, self.prepared.widths.len());
                if i >= self.prepared.widths.len() {
                    break;
                }
            }

            let w = self.prepared.widths[i];
            let kind = self.prepared.kinds[i];
            let break_after = breaks_after(kind);

            if !self.has_content {
                if w > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                    self.append_breakable_segment_from(i, 0, &mut on_line);
                } else {
                    self.start_line_at_segment(i, w);
                }
                if break_after {
                    self.pending_break_segment_index = i as i64 + 1;
                    self.pending_break_paint_width = self.line_w - w;
                }
                i += 1;
                continue;
            }

            let new_w = self.line_w + w;
            if new_w > self.fit_limit {
                if break_after {
                    self.append_whole_segment(i, w);
                    let paint = self.line_w - w;
                    self.emit_current_line(i + 1, 0, paint, &mut on_line);
                    i += 1;
                    continue;
                }

                if self.pending_break_segment_index >= 0 {
                    if self.line_end_segment_index as i64 > self.pending_break_segment_index
                        || (self.line_end_segment_index as i64 == self.pending_break_segment_index
                            && self.line_end_grapheme_index > 0)
                    {
                        self.emit_current_line_default(&mut on_line);
                        continue;
                    }
                    let pbs = self.pending_break_segment_index as usize;
                    let pbw = self.pending_break_paint_width;
                    self.emit_current_line(pbs, 0, pbw, &mut on_line);
                    continue;
                }

                if w > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                    self.emit_current_line_default(&mut on_line);
                    self.append_breakable_segment_from(i, 0, &mut on_line);
                    i += 1;
                    continue;
                }

                self.emit_current_line_default(&mut on_line);
                continue;
            }

            self.append_whole_segment(i, w);
            if break_after {
                self.pending_break_segment_index = i as i64 + 1;
                self.pending_break_paint_width = self.line_w - w;
            }
            i += 1;
        }

        if self.has_content {
            self.emit_current_line_default(&mut on_line);
        }
        self.line_count
    }
}

impl<'a> SimpleWalker<'a> {
    fn new(prepared: &'a PreparedLineBreakData) -> Self {
        Self {
            prepared,
            fit_limit: 0.0,
            line_count: 0,
            line_w: 0.0,
            has_content: false,
            line_start_segment_index: 0,
            line_start_grapheme_index: 0,
            line_end_segment_index: 0,
            line_end_grapheme_index: 0,
            pending_break_segment_index: -1,
            pending_break_paint_width: 0.0,
        }
    }
}

// ===== RawWalker (maps walkPreparedLinesRaw's non-simple chunk loop) =====

struct RawWalker<'a> {
    prepared: &'a PreparedLineBreakData,
    fit_limit: f32,
    line_count: usize,
    line_w: f32,
    has_content: bool,
    line_start_segment_index: usize,
    line_start_grapheme_index: usize,
    line_end_segment_index: usize,
    line_end_grapheme_index: usize,
    pending_break_segment_index: i64,
    pending_break_fit_width: f32,
    pending_break_paint_width: f32,
    pending_break_kind: Option<SegmentBreakKind>,
}

impl<'a> RawWalker<'a> {
    fn new(prepared: &'a PreparedLineBreakData) -> Self {
        Self {
            prepared,
            fit_limit: 0.0,
            line_count: 0,
            line_w: 0.0,
            has_content: false,
            line_start_segment_index: 0,
            line_start_grapheme_index: 0,
            line_end_segment_index: 0,
            line_end_grapheme_index: 0,
            pending_break_segment_index: -1,
            pending_break_fit_width: 0.0,
            pending_break_paint_width: 0.0,
            pending_break_kind: None,
        }
    }

    fn clear_pending_break(&mut self) {
        self.pending_break_segment_index = -1;
        self.pending_break_fit_width = 0.0;
        self.pending_break_paint_width = 0.0;
        self.pending_break_kind = None;
    }

    fn get_current_line_paint_width(&self) -> f32 {
        if matches!(self.pending_break_kind, Some(SegmentBreakKind::SoftHyphen))
            && self.pending_break_segment_index == self.line_end_segment_index as i64
            && self.line_end_grapheme_index == 0
        {
            self.pending_break_paint_width
        } else {
            self.line_w
        }
    }

    fn emit_current_line_default(&mut self, on_line: &mut Option<LineVisitor<'_>>) {
        let es = self.line_end_segment_index;
        let eg = self.line_end_grapheme_index;
        let w = self.get_current_line_paint_width();
        self.emit_current_line(es, eg, Some(w), on_line);
    }

    fn emit_current_line(
        &mut self,
        end_segment_index: usize,
        end_grapheme_index: usize,
        width: Option<f32>,
        on_line: &mut Option<LineVisitor<'_>>,
    ) {
        self.line_count += 1;
        let paint_width = match width {
            Some(w) => w,
            None => self.get_current_line_paint_width(),
        };
        if let Some(cb) = on_line.as_deref_mut() {
            cb(
                finalize_line_paint_width(
                    self.prepared,
                    paint_width,
                    self.line_start_segment_index,
                    self.line_start_grapheme_index,
                    end_segment_index,
                    end_grapheme_index,
                ),
                self.line_start_segment_index,
                self.line_start_grapheme_index,
                end_segment_index,
                end_grapheme_index,
            );
        }
        self.line_w = 0.0;
        self.has_content = false;
        self.clear_pending_break();
    }

    fn start_line_at_segment(&mut self, segment_index: usize, width: f32) {
        self.has_content = true;
        self.line_start_segment_index = segment_index;
        self.line_start_grapheme_index = 0;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
        self.line_w = width;
    }

    fn start_line_at_grapheme(
        &mut self,
        segment_index: usize,
        grapheme_index: usize,
        width: f32,
    ) {
        self.has_content = true;
        self.line_start_segment_index = segment_index;
        self.line_start_grapheme_index = grapheme_index;
        self.line_end_segment_index = segment_index;
        self.line_end_grapheme_index = grapheme_index + 1;
        self.line_w = width;
    }

    fn append_whole_segment(&mut self, segment_index: usize, advance: f32) {
        if !self.has_content {
            self.start_line_at_segment(segment_index, advance);
            return;
        }
        self.line_w += advance;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
    }

    fn update_pending_break_for_whole_segment(
        &mut self,
        kind: SegmentBreakKind,
        break_after: bool,
        segment_index: usize,
        segment_width: f32,
        leading_spacing: f32,
        advance: f32,
    ) {
        if !break_after {
            return;
        }
        let fit_advance = get_break_opportunity_fit_contribution(self.prepared, kind, segment_index, leading_spacing);
        let paint_advance = get_line_end_paint_contribution(self.prepared, kind, segment_index, leading_spacing, segment_width);
        self.pending_break_segment_index = segment_index as i64 + 1;
        self.pending_break_fit_width = self.line_w - advance + fit_advance;
        self.pending_break_paint_width = self.line_w - advance + paint_advance;
        self.pending_break_kind = Some(kind);
    }

    fn append_breakable_segment_from(
        &mut self,
        segment_index: usize,
        start_grapheme_index: usize,
        on_line: &mut Option<LineVisitor<'_>>,
    ) {
        let fit_advances = self.prepared.breakable_fit_advances[segment_index]
            .as_ref()
            .expect("breakable segment must carry fit advances");
        let preferred_breaks = self.prepared.breakable_preferred_breaks[segment_index].as_ref();
        let mut preferred_break_index = match preferred_breaks {
            Some(pb) => get_next_preferred_break_index(pb, 0, start_grapheme_index + 1),
            None => usize::MAX,
        };
        let mut last_preferred_break_end: i64 = -1;
        let mut last_preferred_break_width = 0.0;

        let mut g = start_grapheme_index;
        while g < fit_advances.len() {
            let base_gw = fit_advances[g];

            if !self.has_content {
                self.start_line_at_grapheme(segment_index, g, base_gw);
            } else {
                let gw = get_breakable_grapheme_advance(self.prepared, true, base_gw);
                let candidate_paint_width = self.line_w + gw;
                if get_breakable_candidate_fit_width(self.prepared, candidate_paint_width) > self.fit_limit {
                    if let Some(pb) = preferred_breaks {
                        if last_preferred_break_end > start_grapheme_index as i64 {
                            let lpe = last_preferred_break_end as usize;
                            let lpw = last_preferred_break_width;
                            self.emit_current_line(segment_index, lpe, Some(lpw), on_line);
                            g = lpe;
                            preferred_break_index = get_next_preferred_break_index(pb, preferred_break_index, g + 1);
                            last_preferred_break_end = -1;
                            last_preferred_break_width = 0.0;
                            continue;
                        }
                    }
                    self.emit_current_line_default(on_line);
                    self.start_line_at_grapheme(segment_index, g, base_gw);
                } else {
                    self.line_w = candidate_paint_width;
                    self.line_end_segment_index = segment_index;
                    self.line_end_grapheme_index = g + 1;
                }
            }

            let grapheme_end = g + 1;
            if let Some(pb) = preferred_breaks {
                if preferred_break_index < pb.len() && pb[preferred_break_index] == grapheme_end {
                    last_preferred_break_end = grapheme_end as i64;
                    last_preferred_break_width = self.line_w;
                    preferred_break_index += 1;
                }
            }
            g += 1;
        }

        if self.has_content
            && self.line_end_segment_index == segment_index
            && self.line_end_grapheme_index == fit_advances.len()
        {
            self.line_end_segment_index = segment_index + 1;
            self.line_end_grapheme_index = 0;
        }
    }

    fn emit_empty_chunk(
        &mut self,
        chunk: &PreparedChunk,
        on_line: &mut Option<LineVisitor<'_>>,
    ) {
        self.line_count += 1;
        if let Some(cb) = on_line.as_deref_mut() {
            cb(0.0, chunk.start_segment_index, 0, chunk.consumed_end_segment_index, 0);
        }
        self.clear_pending_break();
    }

    fn walk(mut self, max_width: f32, mut on_line: Option<LineVisitor<'_>>) -> usize {
        if self.prepared.widths.is_empty() || self.prepared.chunks.is_empty() {
            return 0;
        }
        self.fit_limit = max_width + get_engine_profile().line_fit_epsilon;

        for chunk_index in 0..self.prepared.chunks.len() {
            // Read chunk snapshot, hold nothing borrowed into the loop body.
            let (cs_start, cs_end, cs_consumed_end) = {
                let chunk = &self.prepared.chunks[chunk_index];
                (chunk.start_segment_index, chunk.end_segment_index, chunk.consumed_end_segment_index)
            };
            if cs_start == cs_end {
                let chunk = self.prepared.chunks[chunk_index].clone();
                self.emit_empty_chunk(&chunk, &mut on_line);
                continue;
            }

            self.has_content = false;
            self.line_w = 0.0;
            self.line_start_segment_index = cs_start;
            self.line_start_grapheme_index = 0;
            self.line_end_segment_index = cs_start;
            self.line_end_grapheme_index = 0;
            self.clear_pending_break();

            let mut i = cs_start;
            while i < cs_end {
                if !self.has_content {
                    i = normalize_line_start_segment_index(self.prepared, i, cs_end);
                    if i >= cs_end {
                        break;
                    }
                }

                let kind = self.prepared.kinds[i];
                let break_after = breaks_after(kind);
                let leading_spacing = get_leading_letter_spacing(self.prepared, self.has_content, i);
                let w = match kind {
                    SegmentBreakKind::Tab => get_tab_advance(self.line_w + leading_spacing, self.prepared.tab_stop_advance),
                    _ => self.prepared.widths[i],
                };
                let advance = leading_spacing + w;
                let fit_advance = get_whole_segment_fit_contribution(self.prepared, kind, i, leading_spacing, w);

                if matches!(kind, SegmentBreakKind::SoftHyphen) {
                    if self.has_content {
                        self.line_end_segment_index = i + 1;
                        self.line_end_grapheme_index = 0;
                        self.pending_break_segment_index = i as i64 + 1;
                        self.pending_break_fit_width = self.line_w + self.prepared.discretionary_hyphen_width;
                        self.pending_break_paint_width = self.line_w + self.prepared.discretionary_hyphen_width;
                        self.pending_break_kind = Some(kind);
                    }
                    i += 1;
                    continue;
                }

                if !self.has_content {
                    if fit_advance > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                        self.append_breakable_segment_from(i, 0, &mut on_line);
                    } else {
                        self.start_line_at_segment(i, w);
                    }
                    self.update_pending_break_for_whole_segment(kind, break_after, i, w, leading_spacing, advance);
                    i += 1;
                    continue;
                }

                let new_fit_w = self.line_w + fit_advance;
                if new_fit_w > self.fit_limit {
                    let current_break_fit_width = self.line_w
                        + get_break_opportunity_fit_contribution(self.prepared, kind, i, leading_spacing);
                    let current_break_paint_width = self.line_w
                        + get_line_end_paint_contribution(self.prepared, kind, i, leading_spacing, w);

                    let pbsi = self.pending_break_segment_index;
                    let pbfw = self.pending_break_fit_width;
                    let pbpw = self.pending_break_paint_width;
                    if matches!(self.pending_break_kind, Some(SegmentBreakKind::SoftHyphen))
                        && get_engine_profile().prefer_early_soft_hyphen_break
                        && pbfw <= self.fit_limit
                    {
                        self.emit_current_line(pbsi as usize, 0, Some(pbpw), &mut on_line);
                        continue;
                    }

                    if break_after && current_break_fit_width <= self.fit_limit {
                        self.append_whole_segment(i, advance);
                        self.emit_current_line(i + 1, 0, Some(current_break_paint_width), &mut on_line);
                        i += 1;
                        continue;
                    }

                    if pbsi >= 0 && pbfw <= self.fit_limit {
                        if self.line_end_segment_index as i64 > pbsi
                            || (self.line_end_segment_index as i64 == pbsi && self.line_end_grapheme_index > 0)
                        {
                            self.emit_current_line_default(&mut on_line);
                            continue;
                        }
                        let next_segment_index = pbsi as usize;
                        self.emit_current_line(next_segment_index, 0, Some(pbpw), &mut on_line);
                        i = next_segment_index;
                        continue;
                    }

                    if fit_advance > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                        self.emit_current_line_default(&mut on_line);
                        self.append_breakable_segment_from(i, 0, &mut on_line);
                        i += 1;
                        continue;
                    }

                    self.emit_current_line_default(&mut on_line);
                    continue;
                }

                self.append_whole_segment(i, advance);
                self.update_pending_break_for_whole_segment(kind, break_after, i, w, leading_spacing, advance);
                i += 1;
            }

            if self.has_content {
                let pbsi = self.pending_break_segment_index;
                let pbpw = self.pending_break_paint_width;
                let final_paint_width = if pbsi == cs_consumed_end as i64 { pbpw } else { self.line_w };
                self.emit_current_line(cs_consumed_end, 0, Some(final_paint_width), &mut on_line);
            }
        }

        self.line_count
    }
}

pub fn walk_prepared_lines_raw(
    prepared: &PreparedLineBreakData,
    max_width: f32,
    on_line: Option<LineVisitor<'_>>,
) -> usize {
    if prepared.simple_line_walk_fast_path {
        SimpleWalker::new(prepared).walk(max_width, on_line)
    } else {
        RawWalker::new(prepared).walk(max_width, on_line)
    }
}

// ===== ChunkStepper (port of stepPreparedChunkLineGeometry) =====
//
// Returns `Some(width)` when the next line completes (advancing the cursor to
// the start of the following line) and `None` when the chunk is exhausted.
// The TS source mutates `cursor` directly and threads mutable state through
// nested closures; the Rust port escalates those closures to methods.

struct ChunkStepper<'a> {
    prepared: &'a PreparedLineBreakData,
    cursor: &'a mut LineBreakCursor,
    chunk_end_segment_index: usize,
    chunk_consumed_end_segment_index: usize,
    fit_limit: f32,
    line_start_segment_index: usize,
    line_start_grapheme_index: usize,
    line_w: f32,
    has_content: bool,
    line_end_segment_index: usize,
    line_end_grapheme_index: usize,
    pending_break_segment_index: i64,
    pending_break_fit_width: f32,
    pending_break_paint_width: f32,
    pending_break_kind: Option<SegmentBreakKind>,
}

impl<'a> ChunkStepper<'a> {
    fn run(prepared: &'a PreparedLineBreakData, cursor: &'a mut LineBreakCursor, chunk_index: usize, max_width: f32) -> Option<f32> {
        let chunk = &prepared.chunks[chunk_index];
        if chunk.start_segment_index == chunk.end_segment_index {
            cursor.segment_index = chunk.consumed_end_segment_index;
            cursor.grapheme_index = 0;
            return Some(0.0);
        }
        let fit_limit = max_width + get_engine_profile().line_fit_epsilon;
        let line_start_segment_index = cursor.segment_index;
        let line_start_grapheme_index = cursor.grapheme_index;
        let initial_end_segment_index = cursor.segment_index;
        let initial_end_grapheme_index = cursor.grapheme_index;
        let mut s = Self {
            prepared,
            cursor,
            chunk_end_segment_index: chunk.end_segment_index,
            chunk_consumed_end_segment_index: chunk.consumed_end_segment_index,
            fit_limit,
            line_start_segment_index,
            line_start_grapheme_index,
            line_w: 0.0,
            has_content: false,
            line_end_segment_index: initial_end_segment_index,
            line_end_grapheme_index: initial_end_grapheme_index,
            pending_break_segment_index: -1,
            pending_break_fit_width: 0.0,
            pending_break_paint_width: 0.0,
            pending_break_kind: None,
        };
        s.drive()
    }

    fn get_current_line_paint_width(&self) -> f32 {
        if matches!(self.pending_break_kind, Some(SegmentBreakKind::SoftHyphen))
            && self.pending_break_segment_index == self.line_end_segment_index as i64
            && self.line_end_grapheme_index == 0
        {
            self.pending_break_paint_width
        } else {
            self.line_w
        }
    }

    /// `finishLine` — set cursor, return finalized paint width if content was
    /// emitted; `None` when the chunk never produced a line.
    fn finish_line(&mut self, end_segment_index: usize, end_grapheme_index: usize, width: f32) -> Option<f32> {
        if !self.has_content {
            return None;
        }
        self.cursor.segment_index = end_segment_index;
        self.cursor.grapheme_index = end_grapheme_index;
        Some(finalize_line_paint_width(
            self.prepared,
            width,
            self.line_start_segment_index,
            self.line_start_grapheme_index,
            end_segment_index,
            end_grapheme_index,
        ))
    }

    fn finish_line_default(&mut self) -> Option<f32> {
        let es = self.line_end_segment_index;
        let eg = self.line_end_grapheme_index;
        let w = self.get_current_line_paint_width();
        self.finish_line(es, eg, w)
    }

    fn start_line_at_segment(&mut self, segment_index: usize, width: f32) {
        self.has_content = true;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
        self.line_w = width;
    }

    fn start_line_at_grapheme(&mut self, segment_index: usize, grapheme_index: usize, width: f32) {
        self.has_content = true;
        self.line_end_segment_index = segment_index;
        self.line_end_grapheme_index = grapheme_index + 1;
        self.line_w = width;
    }

    fn append_whole_segment(&mut self, segment_index: usize, advance: f32) {
        if !self.has_content {
            self.start_line_at_segment(segment_index, advance);
            return;
        }
        self.line_w += advance;
        self.line_end_segment_index = segment_index + 1;
        self.line_end_grapheme_index = 0;
    }

    fn update_pending_break_for_whole_segment(
        &mut self,
        kind: SegmentBreakKind,
        break_after: bool,
        segment_index: usize,
        segment_width: f32,
        leading_spacing: f32,
        advance: f32,
    ) {
        if !break_after {
            return;
        }
        let fit_advance = get_break_opportunity_fit_contribution(self.prepared, kind, segment_index, leading_spacing);
        let paint_advance = get_line_end_paint_contribution(self.prepared, kind, segment_index, leading_spacing, segment_width);
        self.pending_break_segment_index = segment_index as i64 + 1;
        self.pending_break_fit_width = self.line_w - advance + fit_advance;
        self.pending_break_paint_width = self.line_w - advance + paint_advance;
        self.pending_break_kind = Some(kind);
    }

    /// `appendBreakableSegmentFrom` — returns `Some(line_width)` when folding
    /// forces a break, `None` when the segment fits and continues.
    fn append_breakable_segment_from(&mut self, segment_index: usize, start_grapheme_index: usize) -> Option<f32> {
        let fit_advances = self.prepared.breakable_fit_advances[segment_index]
            .as_ref()
            .expect("breakable segment must carry fit advances");
        let preferred_breaks = self.prepared.breakable_preferred_breaks[segment_index].as_ref();
        let mut preferred_break_index = match preferred_breaks {
            Some(pb) => get_next_preferred_break_index(pb, 0, start_grapheme_index + 1),
            None => usize::MAX,
        };
        let mut last_preferred_break_end: i64 = -1;
        let mut last_preferred_break_width = 0.0;

        let mut g = start_grapheme_index;
        while g < fit_advances.len() {
            let base_gw = fit_advances[g];

            if !self.has_content {
                self.start_line_at_grapheme(segment_index, g, base_gw);
            } else {
                let gw = get_breakable_grapheme_advance(self.prepared, true, base_gw);
                let candidate_paint_width = self.line_w + gw;
                if get_breakable_candidate_fit_width(self.prepared, candidate_paint_width) > self.fit_limit {
                    if let Some(pb) = preferred_breaks {
                        if last_preferred_break_end > start_grapheme_index as i64 {
                            return self.finish_line(segment_index, last_preferred_break_end as usize, last_preferred_break_width);
                        }
                    }
                    return self.finish_line_default();
                }
                self.line_w = candidate_paint_width;
                self.line_end_segment_index = segment_index;
                self.line_end_grapheme_index = g + 1;
            }

            let grapheme_end = g + 1;
            if let Some(pb) = preferred_breaks {
                if preferred_break_index < pb.len() && pb[preferred_break_index] == grapheme_end {
                    last_preferred_break_end = grapheme_end as i64;
                    last_preferred_break_width = self.line_w;
                    preferred_break_index += 1;
                }
            }
            g += 1;
        }

        if self.has_content
            && self.line_end_segment_index == segment_index
            && self.line_end_grapheme_index == fit_advances.len()
        {
            self.line_end_segment_index = segment_index + 1;
            self.line_end_grapheme_index = 0;
        }
        None
    }

    fn maybe_finish_at_soft_hyphen(&mut self) -> Option<f32> {
        if !matches!(self.pending_break_kind, Some(SegmentBreakKind::SoftHyphen))
            || self.pending_break_segment_index < 0
        {
            return None;
        }
        if self.pending_break_fit_width <= self.fit_limit {
            return self.finish_line(self.pending_break_segment_index as usize, 0, self.pending_break_paint_width);
        }
        None
    }

    fn drive(&mut self) -> Option<f32> {
        let _ = get_engine_profile(); // ensure engine is initialised
        let start_segment_index = self.line_start_segment_index;
        let start_grapheme_index = self.line_start_grapheme_index;
        let mut i = start_segment_index;
        while i < self.chunk_end_segment_index {
            let kind = self.prepared.kinds[i];
            let break_after = breaks_after(kind);
            let start_grapheme_index_for_i = if i == start_segment_index { start_grapheme_index } else { 0 };
            let leading_spacing = get_leading_letter_spacing(self.prepared, self.has_content, i);
            let w = match kind {
                SegmentBreakKind::Tab => get_tab_advance(self.line_w + leading_spacing, self.prepared.tab_stop_advance),
                _ => self.prepared.widths[i],
            };
            let advance = leading_spacing + w;
            let fit_advance = get_whole_segment_fit_contribution(self.prepared, kind, i, leading_spacing, w);

            if matches!(kind, SegmentBreakKind::SoftHyphen) && start_grapheme_index_for_i == 0 {
                if self.has_content {
                    self.line_end_segment_index = i + 1;
                    self.line_end_grapheme_index = 0;
                    self.pending_break_segment_index = i as i64 + 1;
                    self.pending_break_fit_width = self.line_w + self.prepared.discretionary_hyphen_width;
                    self.pending_break_paint_width = self.line_w + self.prepared.discretionary_hyphen_width;
                    self.pending_break_kind = Some(kind);
                }
                // (consuming the SoftHyphen inhibits its own break; i.e. do not
                // record as a wrap opportunity here — see TS line ~1070).
                i += 1;
                continue;
            }

            if !self.has_content {
                if start_grapheme_index_for_i > 0 {
                    if let Some(line) = self.append_breakable_segment_from(i, start_grapheme_index_for_i) {
                        return Some(line);
                    }
                } else if fit_advance > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                    if let Some(line) = self.append_breakable_segment_from(i, 0) {
                        return Some(line);
                    }
                } else {
                    self.start_line_at_segment(i, w);
                }
                self.update_pending_break_for_whole_segment(kind, break_after, i, w, leading_spacing, advance);
                i += 1;
                continue;
            }

            let new_fit_w = self.line_w + fit_advance;
            if new_fit_w > self.fit_limit {
                let current_break_fit_width = self.line_w
                    + get_break_opportunity_fit_contribution(self.prepared, kind, i, leading_spacing);
                let current_break_paint_width = self.line_w
                    + get_line_end_paint_contribution(self.prepared, kind, i, leading_spacing, w);

                // Capture pending-break state up-front so borrow checker is happy
                // when we then call `finish_line` (which borrows self mutably).
                let pbsi = self.pending_break_segment_index;
                let pbfw = self.pending_break_fit_width;
                let pbpw = self.pending_break_paint_width;
                if matches!(self.pending_break_kind, Some(SegmentBreakKind::SoftHyphen))
                    && get_engine_profile().prefer_early_soft_hyphen_break
                    && pbfw <= self.fit_limit
                {
                    return self.finish_line(pbsi as usize, 0, pbpw);
                }

                if let Some(soft_break_line) = self.maybe_finish_at_soft_hyphen() {
                    return Some(soft_break_line);
                }

                if break_after && current_break_fit_width <= self.fit_limit {
                    self.append_whole_segment(i, advance);
                    return self.finish_line(i + 1, 0, current_break_paint_width);
                }

                if pbsi >= 0 && pbfw <= self.fit_limit {
                    if self.line_end_segment_index as i64 > pbsi
                        || (self.line_end_segment_index as i64 == pbsi && self.line_end_grapheme_index > 0)
                    {
                        return self.finish_line_default();
                    }
                    return self.finish_line(pbsi as usize, 0, pbpw);
                }

                if fit_advance > self.fit_limit && self.prepared.breakable_fit_advances[i].is_some() {
                    if let Some(current_line) = self.finish_line_default() {
                        return Some(current_line);
                    }
                    // Unreachable in practice (`hasContent` is true here), but kept
                    // faithful to the TS structure.
                    if let Some(line) = self.append_breakable_segment_from(i, 0) {
                        return Some(line);
                    }
                }

                return self.finish_line_default();
            }

            self.append_whole_segment(i, advance);
            self.update_pending_break_for_whole_segment(kind, break_after, i, w, leading_spacing, advance);
            i += 1;
        }

        // Chunk exhausted — flush final line honoring soft-hyphen carry.
        let pbsi = self.pending_break_segment_index;
        let pbpw = self.pending_break_paint_width;
        if pbsi == self.chunk_consumed_end_segment_index as i64 && self.line_end_grapheme_index == 0 {
            return self.finish_line(self.chunk_consumed_end_segment_index, 0, pbpw);
        }
        let final_w = self.line_w;
        self.finish_line(self.chunk_consumed_end_segment_index, 0, final_w)
    }
}

/// `step_prepared_chunk_line_geometry` — thin wrapper to keep parity with
/// the TS `stepPreparedChunkLineGeometry` public surface.
pub fn step_prepared_chunk_line_geometry(
    prepared: &PreparedLineBreakData,
    cursor: &mut LineBreakCursor,
    chunk_index: usize,
    max_width: f32,
) -> Option<f32> {
    ChunkStepper::run(prepared, cursor, chunk_index, max_width)
}

/// `step_prepared_simple_line_geometry` — port of TS `stepPreparedSimpleLineGeometry`
/// (line-break.ts:1036). Faithful line-by-line transcription. The simple path
/// doesn't use leading letter spacing, discretionary hyphens, or the full break
/// opportunity / paint width split — so it's a standalone function with all state
/// kept as mutable locals (no need for a struct).
pub fn step_prepared_simple_line_geometry(
    prepared: &PreparedLineBreakData,
    cursor: &mut LineBreakCursor,
    max_width: f32,
) -> Option<f32> {
    let line_fit_epsilon = get_engine_profile().line_fit_epsilon;
    let fit_limit = max_width + line_fit_epsilon;

    let mut line_w = 0.0;
    let mut has_content = false;
    let mut line_end_segment_index = cursor.segment_index;
    let mut line_end_grapheme_index = cursor.grapheme_index;
    let mut pending_break_segment_index: i64 = -1;
    let mut pending_break_paint_width = 0.0;

    let widths = &prepared.widths;
    let kinds = &prepared.kinds;
    let breakable_fit_advances = &prepared.breakable_fit_advances;
    let breakable_preferred_breaks = &prepared.breakable_preferred_breaks;

    let mut i = cursor.segment_index;
    while i < widths.len() {
        let kind = kinds[i];
        let break_after = breaks_after(kind);
        let start_grapheme_index = if i == cursor.segment_index { cursor.grapheme_index } else { 0 };
        let breakable_fit_advance = &breakable_fit_advances[i];
        let w = widths[i];

        if !has_content {
            if start_grapheme_index > 0 || (w > fit_limit && breakable_fit_advance.is_some()) {
                let fit_advances = breakable_fit_advance.as_ref().expect("breakable FitAdvances");
                let preferred_breaks = breakable_preferred_breaks[i].as_ref();
                let mut preferred_break_index: i64 = match preferred_breaks {
                    None => -1,
                    Some(pb) => get_next_preferred_break_index(pb, 0, start_grapheme_index + 1) as i64,
                };
                let mut last_preferred_break_end: i64 = -1;
                let mut last_preferred_break_width = 0.0;
                let first_grapheme_width = fit_advances[start_grapheme_index];

                has_content = true;
                line_w = first_grapheme_width;
                line_end_segment_index = i;
                line_end_grapheme_index = start_grapheme_index + 1;
                if let Some(pb) = preferred_breaks {
                    let pbi = preferred_break_index as usize;
                    if pbi < pb.len() && pb[pbi] == line_end_grapheme_index {
                        last_preferred_break_end = line_end_grapheme_index as i64;
                        last_preferred_break_width = line_w;
                        preferred_break_index += 1;
                    }
                }

                let mut g = start_grapheme_index + 1;
                while g < fit_advances.len() {
                    let gw = fit_advances[g];
                    if line_w + gw > fit_limit {
                        if let Some(pb) = preferred_breaks {
                            if last_preferred_break_end > start_grapheme_index as i64 {
                                cursor.segment_index = i;
                                cursor.grapheme_index = last_preferred_break_end as usize;
                                return Some(last_preferred_break_width);
                            }
                        }
                        cursor.segment_index = line_end_segment_index;
                        cursor.grapheme_index = line_end_grapheme_index;
                        return Some(line_w);
                    }
                    line_w += gw;
                    line_end_segment_index = i;
                    line_end_grapheme_index = g + 1;
                    if let Some(pb) = preferred_breaks {
                        let pbi = preferred_break_index as usize;
                        if pbi < pb.len() && pb[pbi] == line_end_grapheme_index {
                            last_preferred_break_end = line_end_grapheme_index as i64;
                            last_preferred_break_width = line_w;
                            preferred_break_index += 1;
                        }
                    }
                    g += 1;
                }

                if line_end_segment_index == i && line_end_grapheme_index == fit_advances.len() {
                    line_end_segment_index = i + 1;
                    line_end_grapheme_index = 0;
                }
            } else {
                has_content = true;
                line_w = w;
                line_end_segment_index = i + 1;
                line_end_grapheme_index = 0;
            }
            if break_after {
                pending_break_segment_index = i as i64 + 1;
                pending_break_paint_width = line_w - w;
            }
            i += 1;
            continue;
        }

        if line_w + w > fit_limit {
            if break_after {
                cursor.segment_index = i + 1;
                cursor.grapheme_index = 0;
                return Some(line_w);
            }
            if pending_break_segment_index >= 0 {
                if line_end_segment_index as i64 > pending_break_segment_index
                    || (line_end_segment_index as i64 == pending_break_segment_index && line_end_grapheme_index > 0)
                {
                    cursor.segment_index = line_end_segment_index;
                    cursor.grapheme_index = line_end_grapheme_index;
                    return Some(line_w);
                }
                cursor.segment_index = pending_break_segment_index as usize;
                cursor.grapheme_index = 0;
                return Some(pending_break_paint_width);
            }
            cursor.segment_index = line_end_segment_index;
            cursor.grapheme_index = line_end_grapheme_index;
            return Some(line_w);
        }

        line_w += w;
        line_end_segment_index = i + 1;
        line_end_grapheme_index = 0;
        if break_after {
            pending_break_segment_index = i as i64 + 1;
            pending_break_paint_width = line_w - w;
        }
        i += 1;
    }

    if !has_content {
        return None;
    }
    cursor.segment_index = line_end_segment_index;
    cursor.grapheme_index = line_end_grapheme_index;
    Some(line_w)
}

/// `step_prepared_line_geometry_from_chunk` — line-break.ts:1161.
pub fn step_prepared_line_geometry_from_chunk(
    prepared: &PreparedLineBreakData,
    cursor: &mut LineBreakCursor,
    chunk_index: usize,
    max_width: f32,
) -> Option<f32> {
    if prepared.simple_line_walk_fast_path {
        step_prepared_simple_line_geometry(prepared, cursor, max_width)
    } else {
        step_prepared_chunk_line_geometry(prepared, cursor, chunk_index, max_width)
    }
}

/// `step_prepared_line_geometry` — line-break.ts:1174.
pub fn step_prepared_line_geometry(
    prepared: &PreparedLineBreakData,
    cursor: &mut LineBreakCursor,
    max_width: f32,
) -> Option<f32> {
    let chunk_index = normalize_prepared_line_start(prepared, cursor);
    if chunk_index < 0 {
        return None;
    }
    step_prepared_line_geometry_from_chunk(prepared, cursor, chunk_index as usize, max_width)
}

/// Result of `measure_prepared_line_geometry` — TS line-break.ts:1184.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineGeometryStats {
    pub line_count: usize,
    pub max_line_width: f32,
}

/// `measure_prepared_line_geometry` — line-break.ts:1184.
pub fn measure_prepared_line_geometry(
    prepared: &PreparedLineBreakData,
    max_width: f32,
) -> LineGeometryStats {
    if prepared.widths.is_empty() {
        return LineGeometryStats { line_count: 0, max_line_width: 0.0 };
    }
    let mut cursor = LineBreakCursor { segment_index: 0, grapheme_index: 0 };
    let mut line_count = 0usize;
    let mut max_line_width = 0.0;

    if !prepared.simple_line_walk_fast_path {
        let mut chunk_index = normalize_prepared_line_start(prepared, &mut cursor);
        while chunk_index >= 0 {
            let line_width = match step_prepared_chunk_line_geometry(
                prepared,
                &mut cursor,
                chunk_index as usize,
                max_width,
            ) {
                None => return LineGeometryStats { line_count, max_line_width },
                Some(w) => w,
            };
            line_count += 1;
            if line_width > max_line_width {
                max_line_width = line_width;
            }
            chunk_index = normalize_line_start_chunk_index_from_hint(prepared, chunk_index as usize, &mut cursor);
        }
        return LineGeometryStats { line_count, max_line_width };
    }

    loop {
        let line_width = match step_prepared_line_geometry(prepared, &mut cursor, max_width) {
            None => return LineGeometryStats { line_count, max_line_width },
            Some(w) => w,
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

    fn flat_prepared(widths: Vec<f32>, kinds: Vec<SegmentBreakKind>, simple: bool) -> PreparedLineBreakData {
        let n = widths.len();
        PreparedLineBreakData {
            widths,
            kinds,
            breakable_fit_advances: vec![None; n],
            breakable_preferred_breaks: vec![None; n],
            discretionary_hyphen_width: 0.0,
            line_end_fit_advances: vec![0.0; n],
            line_end_paint_advances: vec![0.0; n],
            letter_spacing: 0.0,
            spacing_grapheme_counts: vec![0; n],
            tab_stop_advance: 0.0,
            chunks: vec![PreparedChunk {
                start_segment_index: 0,
                end_segment_index: n,
                consumed_end_segment_index: n,
            }],
            simple_line_walk_fast_path: simple,
        }
    }

    #[test]
    fn chunk_stepper_breaks_simple_three_segment_line() {
        let prepared = flat_prepared(
            vec![10.0, 10.0, 10.0],
            vec![SegmentBreakKind::Text, SegmentBreakKind::Space, SegmentBreakKind::Text],
            false,
        );
        // One chunk [0..3] consumed 3; maxWidth=15 -> two lines (Text+Space split).
        let mut cursor = LineBreakCursor { segment_index: 0, grapheme_index: 0 };
        let w1 = step_prepared_chunk_line_geometry(&prepared, &mut cursor, 0, 15.0);
        assert_eq!(w1, Some(10.0));
        assert_eq!(cursor.segment_index, 2);
        assert_eq!(cursor.grapheme_index, 0);
        // Second chunk call (same chunk) resumes at segment 2.
        let w2 = step_prepared_chunk_line_geometry(&prepared, &mut cursor, 0, 15.0);
        assert_eq!(w2, Some(10.0));
        assert_eq!(cursor.segment_index, 3);
        assert_eq!(cursor.grapheme_index, 0);
        // No more lines from this cursor.
        let w3 = step_prepared_chunk_line_geometry(&prepared, &mut cursor, 0, 15.0);
        assert_eq!(w3, None);
    }

    #[test]
    fn chunk_stepper_unbreakable_overflow_folds_at_pending_break() {
        let prepared = flat_prepared(
            vec![10.0, 10.0, 10.0, 10.0],
            vec![SegmentBreakKind::Text, SegmentBreakKind::Space, SegmentBreakKind::Text, SegmentBreakKind::Text],
            false,
        );
        let mut cursor = LineBreakCursor { segment_index: 0, grapheme_index: 0 };
        // max 15: Text(10) | Space(10), pending break at 2; Text(10) won't fit (20>15) -> break
        // at pending seg 2 with paint width 10.
        let w1 = step_prepared_chunk_line_geometry(&prepared, &mut cursor, 0, 15.0);
        assert_eq!(w1, Some(10.0));
        assert_eq!(cursor.segment_index, 2);
        let w2 = step_prepared_chunk_line_geometry(&prepared, &mut cursor, 0, 15.0);
        // segment 2 Text (10, fits), pad; segment 3 Text (10): line_w=20, doesn't fit.
        // Whole append → flush at end: cursor=4, lineWidth=20 (since both fit individually)
        assert_eq!(w2, Some(20.0));
    }

    #[test]
    fn simple_path_matches_chunk_path_for_flat_text() {
        let kinds = vec![SegmentBreakKind::Text, SegmentBreakKind::Space, SegmentBreakKind::Text];
        let simple = flat_prepared(vec![10.0, 10.0, 10.0], kinds.clone(), true);
        let chunk = flat_prepared(vec![10.0, 10.0, 10.0], kinds, false);
        // Both walkers should report 2 lines, max width 10.
        let s = measure_prepared_line_geometry(&simple, 15.0);
        let c = measure_prepared_line_geometry(&chunk, 15.0);
        assert_eq!(s.line_count, 2, "simple path expected 2 lines, got {}", s.line_count);
        assert_eq!(c.line_count, 2, "chunk path expected 2 lines, got {}", c.line_count);
        assert!((s.max_line_width - 10.0).abs() < 1e-6);
        assert!((c.max_line_width - 10.0).abs() < 1e-6);
    }

    #[test]
    fn line_geometry_stats_empty_prepared() {
        let empty = PreparedLineBreakData {
            widths: vec![],
            kinds: vec![],
            breakable_fit_advances: vec![],
            breakable_preferred_breaks: vec![],
            discretionary_hyphen_width: 0.0,
            line_end_fit_advances: vec![],
            line_end_paint_advances: vec![],
            letter_spacing: 0.0,
            spacing_grapheme_counts: vec![],
            tab_stop_advance: 0.0,
            chunks: vec![],
            simple_line_walk_fast_path: false,
        };
        let stats = measure_prepared_line_geometry(&empty, 100.0);
        assert_eq!(stats.line_count, 0);
        assert!(stats.max_line_width.abs() < 1e-6);
    }
}
