//! `sonnetSemantic.ts` (100 lines) — compiler-grade 1:1 Rust port.
//!
//! Produces lossless semantic segments while mapping display offsets to
//! parser-derived grapheme timing.
//!
//! Offsets are UTF-8 **byte** offsets (Rust-idiomatic). For BMP-only lyric text
//! — the realistic sonnet input — these coincide numerically with the TS UTF-16
//! code-unit offsets. Astral-plane input would diverge in numeric offset value
//! but produce byte-identical segment text + grapheme-time alignment.

use std::sync::OnceLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::lyricstyles::sonnet_v2::types::{GraphemeTiming, Line, SonnetSemanticSegment};
use crate::lyricstyles::sonnet_v2::lyrics_util::grapheme_timing::{
    build_line_grapheme_timeline, split_lyric_graphemes,
};

/// folia `sonnetSemantic.ts` — `PUNCTUATION_ONLY = /^[\s\p{P}\p{S}]+$/u`.
///
/// `true` iff `text` consists entirely of whitespace, Unicode punctuation,
/// or Unicode symbol characters.
fn punctuation_only_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[\s\p{P}\p{S}]+$").unwrap())
}

fn is_punctuation_only(text: &str) -> bool {
    !text.is_empty() && punctuation_only_re().is_match(text)
}

/// folia `sonnetSemantic.ts` — `/^\s+$/u`.
fn is_whitespace_only(text: &str) -> bool {
    !text.is_empty() && text.chars().all(char::is_whitespace)
}

/// A segmenter part mirroring the `Intl.Segmenter('word')` return shape.
struct SegmenterPart {
    segment: String,
    /// UTF-8 byte offset into the original text (corresponds to TS `index`).
    index: usize,
    /// Mirrors `part.isWordLike`. When the segmenter reports it (browser
    /// `Intl.Segmenter`), use that. Otherwise derive via `!PUNCTUATION_ONLY.test`.
    is_word_like: bool,
}

/// folia `sonnetSemantic.ts` — `getSegmenterParts(text)`.
///
/// In the browser path, `Intl.Segmenter(undefined, { granularity: 'word' })`
/// is always present, so folia effectively only takes the word-segmentation arm.
/// Rust uses `UnicodeSegmentation::split_word_bounds`, which splits at each
/// UAX#29 word boundary — segmentally identical to `Intl.Segmenter('word')`.
/// `isWordLike` is derived via `!PUNCTUATION_ONLY.test(segment)` (same rule as
/// folia's grapheme fallback).
fn get_segmenter_parts(text: &str) -> Vec<SegmenterPart> {
    let mut cursor = 0_usize; // UTF-8 byte offset, mirroring JS `cursor += segment.length`.
    let mut out = Vec::new();
    for segment in text.split_word_bounds() {
        let index = cursor;
        cursor += segment.len();
        let is_word_like = !is_punctuation_only(segment);
        out.push(SegmenterPart {
            segment: segment.to_string(),
            index,
            is_word_like,
        });
    }
    out
}

/// One grapheme range, byte offsets inclusive of `start`, exclusive of `end`.
struct GraphemeRange {
    start: usize,
    end: usize,
}

/// folia `sonnetSemantic.ts` — `getGraphemeRanges(text)`.
fn get_grapheme_ranges(text: &str) -> Vec<GraphemeRange> {
    let mut cursor = 0_usize;
    let mut out = Vec::new();
    for grapheme in split_lyric_graphemes(text) {
        let start = cursor;
        let end = cursor + grapheme.len();
        cursor = end;
        out.push(GraphemeRange { start, end });
    }
    out
}

/// Aggregated timing for a segment.
struct TimingForRange {
    graphemes: Vec<GraphemeTiming>,
    word_indices: Vec<usize>,
    start_time: f64,
    end_time: f64,
}

/// folia `sonnetSemantic.ts` — `timingForRange(line, startOffset, endOffset, timeline, ranges)`.
fn timing_for_range(
    line: &Line,
    start_offset: usize,
    end_offset: usize,
    timeline: &[GraphemeTiming],
    ranges: &[GraphemeRange],
) -> TimingForRange {
    // `indices = ranges.flatMap((range, index) => (range.end > startOffset && range.start < endOffset ? [index] : []))`
    let indices: Vec<usize> = ranges
        .iter()
        .enumerate()
        .filter(|(_, r)| r.end > start_offset && r.start < end_offset)
        .map(|(i, _)| i)
        .collect();

    // `graphemes = indices.map(index => timeline[index]).filter(Boolean)`
    let mut graphemes: Vec<GraphemeTiming> = Vec::new();
    for i in &indices {
        if let Some(g) = timeline.get(*i) {
            graphemes.push(g.clone());
        }
    }

    // `wordIndices = [...new Set(graphemes.flatMap(item => (typeof item.wordIndex === 'number' ? [item.wordIndex] : [])))]`
    let mut word_indices: Vec<usize> = Vec::new();
    for g in &graphemes {
        if let Some(w) = g.word_index {
            if !word_indices.contains(&w) {
                word_indices.push(w);
            }
        }
    }

    // `startTime = graphemes[0]?.startTime ?? line.startTime`
    let start_time = graphemes
        .first()
        .map(|g| g.start_time)
        .unwrap_or(line.start_time);
    // `endTime = graphemes[graphemes.length - 1]?.endTime ?? line.endTime`
    let end_time = graphemes
        .last()
        .map(|g| g.end_time)
        .unwrap_or(line.end_time);

    TimingForRange {
        graphemes,
        word_indices,
        start_time,
        end_time,
    }
}

/// folia `sonnetSemantic.ts` — `buildSonnetSemanticSegments(line)`.
///
/// Produces lossless semantic segments while mapping display offsets to
/// parser-derived grapheme timing. Sticky-merges non-word-like (punctuation
/// glue) runs into the preceding word segment, preserving whitespace segments
/// as their own entries.
pub fn build_sonnet_semantic_segments(line: &Line) -> Vec<SonnetSemanticSegment> {
    if line.full_text.is_empty() {
        return Vec::new();
    }
    let timeline = build_line_grapheme_timeline(line);
    let ranges = get_grapheme_ranges(&line.full_text);
    let parts = get_segmenter_parts(&line.full_text);

    // First pass: build raw segments from the segmenter parts. Each segment's
    // text range is `[part.index, parts[i+1].index ?? line.fullText.length]`.
    let mut segments: Vec<SonnetSemanticSegment> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        let start_offset = part.index;
        let end_offset = parts
            .get(i + 1)
            .map(|p| p.index)
            .unwrap_or(line.full_text.len());
        let text = line.full_text[start_offset..end_offset].to_string();
        let timing = timing_for_range(line, start_offset, end_offset, &timeline, &ranges);
        let is_word_like = part.is_word_like || !is_punctuation_only(&part.segment);
        segments.push(SonnetSemanticSegment {
            text,
            start_offset,
            end_offset,
            start_time: timing.start_time,
            end_time: timing.end_time,
            word_indices: timing.word_indices,
            graphemes: timing.graphemes,
            is_word_like,
        });
    }

    // Second pass: sticky-merge punctuation glue into the preceding segment.
    // `if (previous && !segment.isWordLike && !/^\s+$/u.test(segment.text))`
    let mut sticky: Vec<SonnetSemanticSegment> = Vec::with_capacity(segments.len());
    for segment in segments {
        let prev_last = sticky.len().checked_sub(1);
        if let Some(pi) = prev_last {
            let prev = &mut sticky[pi];
            if !segment.is_word_like && !is_whitespace_only(&segment.text) {
                prev.text.push_str(&segment.text);
                prev.end_offset = segment.end_offset;
                prev.end_time = prev.end_time.max(segment.end_time);
                prev.graphemes.extend(segment.graphemes);
                // `wordIndices = [...new Set([...prev.wordIndices, ...segment.wordIndices])]`
                let mut seen: std::collections::HashSet<usize> =
                    prev.word_indices.iter().copied().collect();
                for w in segment.word_indices {
                    if seen.insert(w) {
                        prev.word_indices.push(w);
                    }
                }
                continue;
            }
        }
        // `sticky.push({ ...segment, graphemes: [...segment.graphemes], wordIndices: [...segment.wordIndices] })`
        // — semantically a defensive shallow copy; the owned `segment` already
        // provides unique ownership.
        sticky.push(segment);
    }
    sticky
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::Line;

    fn line(start: f64, end: f64, text: &str) -> Line {
        Line {
            words: Vec::new(),
            start_time: start,
            end_time: end,
            full_text: text.to_string(),
            render_hints: None,
            block_index: None,
            song_part: None,
            is_chorus: false,
        }
    }

    #[test]
    fn empty_full_text_returns_empty() {
        let l = line(0.0, 1.0, "");
        assert!(build_sonnet_semantic_segments(&l).is_empty());
    }

    #[test]
    fn plain_word_yields_single_word_like_segment() {
        let l = line(0.0, 1.0, "hello");
        let segs = build_sonnet_semantic_segments(&l);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hello");
        assert!(segs[0].is_word_like);
        assert_eq!(segs[0].start_offset, 0);
        assert_eq!(segs[0].end_offset, 5);
    }

    #[test]
    fn punctuation_glue_sticky_merges_into_preceding_word() {
        // "hello," — comma is punctuation-only and not whitespace-only,
        // so it must be merged into "hello" yielding a single segment "hello,".
        let l = line(0.0, 1.0, "hello,");
        let segs = build_sonnet_semantic_segments(&l);
        assert_eq!(segs.len(), 1, "expected glue merged into preceding word");
        assert_eq!(segs[0].text, "hello,");
        assert!(segs[0].is_word_like);
    }

    #[test]
    fn whitespace_segment_kept_separately() {
        // "hello world" — space-only segment must NOT be merged (whitespace check).
        let l = line(0.0, 1.0, "hello world");
        let segs = build_sonnet_semantic_segments(&l);
        // Expected: ["hello", " ", "world"] — three segments.
        assert_eq!(segs.len(), 3, "segments = {:?}", segs);
        assert_eq!(segs[0].text, "hello");
        assert_eq!(segs[1].text, " ");
        assert!(!segs[1].is_word_like);
        assert_eq!(segs[2].text, "world");
    }

    #[test]
    fn cjk_characters_split_per_grapheme() {
        // `Intl.Segmenter('word')` segments CJK characters individually
        // (no inter-CJK word boundaries). Rust `split_word_bounds` mirrors
        // this: each CJK glyph becomes its own segment.
        let l = line(0.0, 1.0, "你好");
        let segs = build_sonnet_semantic_segments(&l);
        assert_eq!(segs.len(), 2, "segments = {:?}", segs);
        assert_eq!(segs[0].text, "你");
        assert_eq!(segs[1].text, "好");
        assert!(segs[0].is_word_like);
        assert!(segs[1].is_word_like);
    }
}
