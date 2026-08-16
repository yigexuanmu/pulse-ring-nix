//! Folia sonnet v2 — pretext `line-text.ts` (107 lines) `compiler-grade 1:1 port`.
//!
//! Builds the visible text of a line range from a `PreparedTextWithSegments`
//! (segments + kinds + start/end indices) by stitching per-segment strings,
//! skipping soft-hyphen and hard-break sentinels, and emitting a literal `-`
//! when a discretionary (soft) hyphen terminates the segment immediately
//! before the line break.
//!
//! ## Algorithm faithfulness
//!
//! pretext TS uses `Intl.Segmenter('grapheme')` for per-segment grapheme
//! slicing when the line starts/ends mid-segment. Rust port uses
//! `unicode-segmentation::graphemes(..., true)` which implements UAX#29
//! grapheme cluster boundaries (same Unicode standard). The segment walker
//! is byte-faithful otherwise — `soft-hyphen` / `hard-break` `kind` sentinels
//! are filtered identically, and the discretionary-hyphen emit rule is
//! preserved exactly.
//!
//! ## Cache
//!
//! pretext stores per-`PreparedTextWithSegments` grapheme caches in a
//! `WeakMap`. Rust port exposes the same shape: the caller owns a
//! `LineTextCache` for each `PreparedTextWithSegments`. There is no global
//! cache; lifetime of the segment grapheme cache equals the owning prepared
//! text.

use super::analysis::SegmentBreakKind;
use std::cell::RefCell;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// Caller-owned grapheme cache (replaces pretext's WeakMap scope since Rust
/// has no GC; the prepared text owns this struct in its lifetime).
#[derive(Default)]
pub struct LineTextCache {
    /// Segment index -> graphemes slice cache.
    pub segment_graphemes: RefCell<HashMap<usize, Vec<String>>>,
}

/// Minimal prepared input shape `buildLineTextFromRange` consumes. The full
/// `PreparedTextWithSegments` will be defined in `layout.rs` (Phase 2.6) and
/// will contain these as fields; we accept a borrow here for decoupling.
pub struct PreparedSegmentView<'a> {
    pub segments: &'a [String],
    pub kinds: &'a [SegmentBreakKind],
}

/// `getSegmentGraphemes` — pretext line-text.ts:14. Caches per-segment
/// grapheme slices (UAX#29 cluster boundaries) keyed by segment index.
pub fn get_segment_graphemes(
    segment_index: usize,
    segments: &[String],
    cache: &LineTextCache,
) -> Vec<String> {
    if let Some(g) = cache.segment_graphemes.borrow().get(&segment_index) {
        return g.clone();
    }
    let text = segments.get(segment_index).map(String::as_str).unwrap_or("");
    let graphemes: Vec<String> = UnicodeSegmentation::graphemes(text, true)
        .map(String::from)
        .collect();
    cache
        .segment_graphemes
        .borrow_mut()
        .insert(segment_index, graphemes.clone());
    graphemes
}

/// `lineHasDiscretionaryHyphen` — pretext line-text.ts:32. The line ends with
/// a discretionary hyphen when `endSegmentIndex > startSegmentIndex` AND the
/// immediately-preceding break kind is `soft-hyphen`.
pub fn line_has_discretionary_hyphen(
    kinds: &[SegmentBreakKind],
    start_segment_index: usize,
    end_segment_index: usize,
) -> bool {
    end_segment_index > start_segment_index
        && kinds.get(end_segment_index - 1) == Some(&SegmentBreakKind::SoftHyphen)
}

/// `appendSegmentGraphemeRange` — pretext line-text.ts:43. Stitches graphemes
/// `[start, end)` into the running text buffer.
fn append_segment_grapheme_range(
    text: &mut String,
    graphemes: &[String],
    start_grapheme_index: usize,
    end_grapheme_index: usize,
) {
    let end = end_grapheme_index.min(graphemes.len());
    for i in start_grapheme_index..end {
        text.push_str(&graphemes[i]);
    }
}

/// `buildLineTextFromRange` — pretext line-text.ts:55.
///
/// Reconstructs the visible line text from the segment range
/// `[startSegmentIndex, endSegmentIndex)`, skipping `soft-hyphen` /
/// `hard-break` sentinel segments, and appending `-` when the line ends at a
/// discretionary boundary or grapheme sub-slicing is needed at the final
/// segment.
#[allow(clippy::too_many_arguments)]
pub fn build_line_text_from_range(
    prepared: &PreparedSegmentView<'_>,
    cache: &LineTextCache,
    start_segment_index: usize,
    start_grapheme_index: usize,
    end_segment_index: usize,
    end_grapheme_index: usize,
) -> String {
    let mut text = String::new();
    let ends_with_discretionary_hyphen = line_has_discretionary_hyphen(
        prepared.kinds,
        start_segment_index,
        end_segment_index,
    );

    for i in start_segment_index..end_segment_index {
        let kind = prepared.kinds.get(i);
        if kind == Some(&SegmentBreakKind::SoftHyphen)
            || kind == Some(&SegmentBreakKind::HardBreak)
        {
            continue;
        }
        if i == start_segment_index && start_grapheme_index > 0 {
            let graphemes = get_segment_graphemes(i, prepared.segments, cache);
            append_segment_grapheme_range(
                &mut text,
                &graphemes,
                start_grapheme_index,
                graphemes.len(),
            );
        } else {
            // TS fallback: text += prepared.segments[i]
            if let Some(s) = prepared.segments.get(i) {
                text.push_str(s);
            }
        }
    }

    if end_grapheme_index > 0 {
        if ends_with_discretionary_hyphen {
            text.push('-');
        }
        let graphemes = get_segment_graphemes(end_segment_index, prepared.segments, cache);
        let gstart = if start_segment_index == end_segment_index {
            start_grapheme_index
        } else {
            0
        };
        append_segment_grapheme_range(&mut text, &graphemes, gstart, end_grapheme_index);
    } else if ends_with_discretionary_hyphen {
        text.push('-');
    }

    text
}

/// `getLineTextCache` — pretext line-text.ts:80. In TS this lazily attaches a
/// Map to the prepared-text WeakMap. Rust has no GC; the caller owns a
/// `LineTextCache` for each `PreparedTextWithSegments` and this helper simply
/// constructs a fresh one if not yet present.
pub fn get_line_text_cache() -> LineTextCache {
    LineTextCache::default()
}

/// `clearLineTextCaches` — pretext line-text.ts:99. Lifetime-bound in Rust;
/// no global cache to reset, present for API surface symmetry.
pub fn clear_line_text_caches(cache: &mut LineTextCache) {
    cache.segment_graphemes.borrow_mut().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_discretionary_hyphen_true_when_soft_hyphen_terminates() {
        let kinds = vec![SegmentBreakKind::Text, SegmentBreakKind::SoftHyphen];
        assert!(line_has_discretionary_hyphen(&kinds, 0, 2));
    }

    #[test]
    fn line_has_discretionary_hyphen_false_when_empty_range() {
        assert!(!line_has_discretionary_hyphen(&[SegmentBreakKind::Text], 0, 0));
    }

    #[test]
    fn line_has_discretionary_hyphen_false_when_not_soft_hyphen() {
        assert!(!line_has_discretionary_hyphen(
            &[SegmentBreakKind::Text, SegmentBreakKind::Space],
            0,
            2
        ));
    }

    #[test]
    fn build_line_text_simple_text_segs() {
        let segments: Vec<String> = vec!["abc".into(), " ".into(), "def".into()];
        let kinds = vec![
            SegmentBreakKind::Text,
            SegmentBreakKind::Space,
            SegmentBreakKind::Text,
        ];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let out = build_line_text_from_range(&view, &cache, 0, 0, 3, 0);
        assert_eq!(out, "abc def");
    }

    #[test]
    fn build_line_text_skips_soft_hyphen_appends_dash() {
        // segments = ["abc","\u{00AD}","def"], kinds = [Text, SoftHyphen, Text]
        // range start=0 end=2 (Text + SoftHyphen); SoftHyphen skipped in the for
        // loop, ends_with_discretionary_hyphen=true => suffix '-'.
        let segments: Vec<String> = vec!["abc".into(), "\u{00AD}".into(), "def".into()];
        let kinds = vec![
            SegmentBreakKind::Text,
            SegmentBreakKind::SoftHyphen,
            SegmentBreakKind::Text,
        ];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let out = build_line_text_from_range(&view, &cache, 0, 0, 2, 0);
        assert_eq!(out, "abc-");
    }

    #[test]
    fn build_line_text_skips_hard_break() {
        // HardBreak sentinel is filtered from the visible text; surrounding Text
        // segments keep their content (no joining character is inserted).
        let segments: Vec<String> = vec!["abc".into(), "\n".into(), "def".into()];
        let kinds = vec![
            SegmentBreakKind::Text,
            SegmentBreakKind::HardBreak,
            SegmentBreakKind::Text,
        ];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let out = build_line_text_from_range(&view, &cache, 0, 0, 3, 0);
        // SoftHyphen emit rule does not apply; HardBreak segment is skipped;
        // result is "abc" immediately followed by "def" with no separator.
        assert_eq!(out, "abcdef");
    }

    #[test]
    fn build_line_text_grapheme_subslice_at_start_grapheme_index() {
        // Single Text segment "abc"; start_grapheme_index=1 -> skip leading 'a'.
        let segments: Vec<String> = vec!["abc".into()];
        let kinds = vec![SegmentBreakKind::Text];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let out = build_line_text_from_range(&view, &cache, 0, 1, 1, 0);
        assert_eq!(out, "bc");
    }

    #[test]
    fn build_line_text_grapheme_subslice_at_end_grapheme_index() {
        // Two segments ["abc","def"]; end_segment_index=1 with end_grapheme_index=2
        // -> take first 2 graphemes of "def" = "de".
        let segments: Vec<String> = vec!["abc".into(), "def".into()];
        let kinds = vec![SegmentBreakKind::Text, SegmentBreakKind::Text];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let out = build_line_text_from_range(&view, &cache, 0, 0, 1, 2);
        // start=0 end_seg=1 end_grapheme=2: For-loop covers seg 0 entirely ("abc"),
        // then the if-end_grapheme_index>0 branch appends grapheme range from seg 1
        // (start_grapheme=0 since start_seg != end_seg) graphemes[0..2] = "de".
        assert_eq!(out, "abcde");
    }

    #[test]
    fn get_segment_graphemes_caches_per_index() {
        let segments: Vec<String> = vec!["abc".into()];
        let kinds = vec![SegmentBreakKind::Text];
        let view = PreparedSegmentView { segments: &segments, kinds: &kinds };
        let cache = get_line_text_cache();
        let g1 = get_segment_graphemes(0, view.segments, &cache);
        let g2 = get_segment_graphemes(0, view.segments, &cache);
        assert_eq!(g1, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(g1, g2);
        // Cache hit should not double-insert.
        assert_eq!(cache.segment_graphemes.borrow().len(), 1);
    }
}
