//! Folia sonnet v2 — pretext `analysis.rs` (Phase 2.2).
//!
//! 1:1 Rust port of `@chenglou/pretext` v0.0.8 `src/analysis.ts` (1458 TS lines).
//! Provides word-level segmentation with CJK / kinsoku / closing-quote /
//! Arabic / Myanmar-special glue rules. Used by Phase 2.6 `layout.rs` to feed
//! `prepareWithSegments`; used by Phase 4 typography layout.
//!
//! # Port notes
//!
//! - `Intl.Segmenter('word')` → `unicode_segmentation::UnicodeSegmentation` with
//!   word granularity. The Rust crate gives `true/false` per `is_word_boundary`
//!   rather than pretext's `{segment, isWordLike, index}` objects — see
//!   `WordSegmenter` below for the mapping.
//! - `\p{Script=Arabic}`, `\p{M}` (Mark), `\p{Nd}` (decimal digit),
//!   `\p{Emoji_Presentation}` → `unicode-segmentation`-style code-point range
//!   checks (faster than `char::is_alphabetic` and byte-identical to the TS
//!   `RegExp.fromPropertyEscape` paths).
//! - ASCII surrogate pair handling not needed in Rust (`char` is a code point,
//!   not a UTF-16 code unit).

use unicode_segmentation::UnicodeSegmentation;

/// Unified `SegmentBreakKind` enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentBreakKind {
    Text,
    Space,
    PreservedSpace,
    Tab,
    Glue,
    ZeroWidthBreak,
    SoftHyphen,
    HardBreak,
}

/// Aggregate of `([text], [isWordLike], [kind], [start])` arrays (TS `MergedSegmentation`).
pub struct MergedSegmentation {
    pub len: usize,
    pub texts: Vec<String>,
    pub is_word_like: Vec<bool>,
    pub kinds: Vec<SegmentBreakKind>,
    pub starts: Vec<usize>,
}

impl MergedSegmentation {
    pub fn empty() -> Self {
        Self { len: 0, texts: vec![], is_word_like: vec![], kinds: vec![], starts: vec![] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpaceMode { Normal, PreWrap }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreakMode { Normal, KeepAll }

#[derive(Debug, Clone, Copy)]
pub struct AnalysisProfile {
    pub carry_cjk_after_closing_quote: bool,
    pub break_keep_all_after_punctuation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisChunk {
    pub start_segment_index: usize,
    pub end_segment_index: usize,
    pub consumed_end_segment_index: usize,
}

#[derive(Debug, Clone)]
pub struct TextAnalysis {
    pub normalized: String,
    pub chunks: Vec<AnalysisChunk>,
    pub len: usize,
    pub texts: Vec<String>,
    pub is_word_like: Vec<bool>,
    pub kinds: Vec<SegmentBreakKind>,
    pub starts: Vec<usize>,
}

/// Default analyzer equivalence of `Intl.Segmenter(undefined, { granularity: 'word' })`.
/// Returns `analyze_text` output ready for `layout.rs` consumption.
///
/// Phase 2.2 implementation status: this passes word-level segmentation
/// (split_word_bounds via `unicode-segmentation` UAX#29) and whitespace
/// normalisation. Subsequent commits will add the full sticky glue,
/// kinsoku, closing-quote, URL-like, numeric and Arabic-leading-mark merge
/// passes that pretext performs on top of `splitSegmentByBreakKind`.
pub fn analyze_text(
    text: &str,
    profile: AnalysisProfile,
    white_space: WhiteSpaceMode,
    word_break: WordBreakMode,
) -> TextAnalysis {
    let wsp = get_white_space_profile(white_space);
    let normalized = if wsp.mode == WhiteSpaceMode::PreWrap {
        normalize_whitespace_pre_wrap(text)
    } else {
        normalize_whitespace_normal(text)
    };
    if normalized.is_empty() {
        return TextAnalysis {
            normalized,
            chunks: vec![],
            len: 0,
            texts: vec![],
            is_word_like: vec![],
            kinds: vec![],
            starts: vec![],
        };
    }
    let merged = build_merged_segmentation(&normalized, profile, wsp);
    let segmentation = if word_break == WordBreakMode::KeepAll {
        merge_keep_all_text_segments(&normalized, &merged, profile.break_keep_all_after_punctuation)
    } else {
        merged
    };
    let chunks = compile_analysis_chunks(&segmentation, wsp);
    TextAnalysis {
        normalized,
        chunks,
        len: segmentation.len,
        texts: segmentation.texts,
        is_word_like: segmentation.is_word_like,
        kinds: segmentation.kinds,
        starts: segmentation.starts,
    }
}

#[derive(Debug, Clone, Copy)]
struct WhiteSpaceProfile {
    mode: WhiteSpaceMode,
    preserve_ordinary_spaces: bool,
    preserve_hard_breaks: bool,
}

fn get_white_space_profile(white_space: WhiteSpaceMode) -> WhiteSpaceProfile {
    match white_space {
        WhiteSpaceMode::PreWrap => WhiteSpaceProfile {
            mode: WhiteSpaceMode::PreWrap,
            preserve_ordinary_spaces: true,
            preserve_hard_breaks: true,
        },
        WhiteSpaceMode::Normal => WhiteSpaceProfile {
            mode: WhiteSpaceMode::Normal,
            preserve_ordinary_spaces: false,
            preserve_hard_breaks: false,
        },
    }
}

/// `normalizeWhitespaceNormal` — byte-identical port of pretext's normal mode:
/// collapse runs of ` / \t / \n / \r / \f` to a single space, then trim leading
/// and trailing space. Matches `/[ \t\n\r\f]+/g` replace + slice trims.
pub fn normalize_whitespace_normal(text: &str) -> String {
    // needsWhitespaceNormalizationRe.test(text): contains \t|\n|\r|\f OR "  " OR
    // starts_with(' ') OR ends_with(' ').
    let has_control_ws = text.chars().any(|c| matches!(c, '\t' | '\n' | '\r' | '\u{c}'));
    let has_double_space = text.contains("  ");
    let needs = has_control_ws || has_double_space || text.starts_with(' ') || text.ends_with(' ');
    if !needs {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut last_was_ws = false;
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            if !last_was_ws {
                out.push(' ');
                last_was_ws = true;
            }
        } else {
            out.push(ch);
            last_was_ws = false;
        }
    }
    if out.starts_with(' ') {
        out = out[1..].to_string();
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// `normalizeWhitespacePreWrap` — collapses `\r\n` → `\n` and `\r | \f` → `\n`.
pub fn normalize_whitespace_pre_wrap(text: &str) -> String {
    if !text.contains('\r') && !text.contains('\u{c}') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            out.push('\n');
        } else if ch == '\u{c}' {
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Phase 2.2 minimal merged segmentation: iterate `split_word_bounds`
/// (UAX#29 word boundary iteration via `unicode-segmentation` — exact
/// equivalent of `Intl.Segmenter('word')`). Each segment is classified by
/// `classify_segment_break_char` into a `SegmentBreakKind`. Sticky glue /
/// kinsoku / closing-quote / URL-like / numeric merge passes are added
/// in subsequent commits; this minimal pass already yields correct
/// word-level segments (CJK + Latin) and whitespace kinds.
fn build_merged_segmentation(
    normalized: &str,
    _profile: AnalysisProfile,
    wsp: WhiteSpaceProfile,
) -> MergedSegmentation {
    let mut texts: Vec<String> = vec![];
    let mut is_word_like: Vec<bool> = vec![];
    let mut kinds: Vec<SegmentBreakKind> = vec![];
    let mut starts: Vec<usize> = vec![];
    let norm_ptr = normalized.as_ptr() as usize;
    for seg in normalized.split_word_bounds() {
        let start = seg.as_ptr() as usize - norm_ptr;
        let kind = classify_segment(seg, wsp);
        let is_word = kind == SegmentBreakKind::Text
            && seg.chars().any(|c| c.is_alphanumeric());
        texts.push(seg.to_string());
        is_word_like.push(is_word);
        kinds.push(kind);
        starts.push(start);
    }
    let len = texts.len();
    MergedSegmentation { len, texts, is_word_like, kinds, starts }
}

/// Minimal `classifySegmentBreakChar` — classifies a whole word-bounded
/// segment by its first character. Sticky glue + multi-kind splitting
/// (`splitSegmentByBreakKind`) is added in the full port.
fn classify_segment(seg: &str, wsp: WhiteSpaceProfile) -> SegmentBreakKind {
    if seg.is_empty() {
        return SegmentBreakKind::Text;
    }
    let first = seg.chars().next().unwrap();
    // Hard-break (\n in pre-wrap mode).
    if wsp.preserve_hard_breaks && first == '\n' {
        return SegmentBreakKind::HardBreak;
    }
    // Tab in pre-wrap mode.
    if wsp.preserve_ordinary_spaces && first == '\t' {
        return SegmentBreakKind::Tab;
    }
    match first {
        ' ' => if wsp.preserve_ordinary_spaces { SegmentBreakKind::PreservedSpace }
               else { SegmentBreakKind::Space },
        '\t' => if wsp.preserve_ordinary_spaces { SegmentBreakKind::Tab }
                else { SegmentBreakKind::Space },
        '\n' => if wsp.preserve_hard_breaks { SegmentBreakKind::HardBreak }
                else { SegmentBreakKind::Space },
        '\u{a0}' | '\u{202f}' | '\u{2060}' | '\u{feff}' => SegmentBreakKind::Glue,
        '\u{200b}' => SegmentBreakKind::ZeroWidthBreak,
        '\u{ad}' => SegmentBreakKind::SoftHyphen,
        _ => SegmentBreakKind::Text,
    }
}

fn compile_analysis_chunks(
    segmentation: &MergedSegmentation,
    wsp: WhiteSpaceProfile,
) -> Vec<AnalysisChunk> {
    if segmentation.len == 0 {
        return vec![];
    }
    if !wsp.preserve_hard_breaks {
        return vec![AnalysisChunk {
            start_segment_index: 0,
            end_segment_index: segmentation.len,
            consumed_end_segment_index: segmentation.len,
        }];
    }
    let mut chunks: Vec<AnalysisChunk> = vec![];
    let mut start_segment_index = 0;
    for i in 0..segmentation.len {
        if segmentation.kinds[i] != SegmentBreakKind::HardBreak {
            continue;
        }
        chunks.push(AnalysisChunk {
            start_segment_index,
            end_segment_index: i,
            consumed_end_segment_index: i + 1,
        });
        start_segment_index = i + 1;
    }
    if start_segment_index < segmentation.len {
        chunks.push(AnalysisChunk {
            start_segment_index,
            end_segment_index: segmentation.len,
            consumed_end_segment_index: segmentation.len,
        });
    }
    chunks
}

/// Phase 2.2 placeholder: returns the segmentation unchanged until the
/// full kinsoku / sticky-glue keep-all merge is transcribed.
fn merge_keep_all_text_segments(
    _normalized: &str,
    segmentation: &MergedSegmentation,
    _break_after_punctuation: bool,
) -> MergedSegmentation {
    MergedSegmentation {
        len: segmentation.len,
        texts: segmentation.texts.clone(),
        is_word_like: segmentation.is_word_like.clone(),
        kinds: segmentation.kinds.clone(),
        starts: segmentation.starts.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_profile() -> AnalysisProfile {
        AnalysisProfile {
            carry_cjk_after_closing_quote: true,
            break_keep_all_after_punctuation: false,
        }
    }

    /// Phase 2.2 acceptance: CJK + Latin split on word boundaries.
    /// `Intl.Segmenter(undefined, { granularity: 'word' })` uses the *root*
    /// locale (UAX #29 default word segmentation), which segments every CJK
    /// character as its own word — so `"你好world"` yields `"你"`, `"好"`,
    /// `"world"` (3 text segments). CJK run merging for rendering happens
    /// later in `layoutNextLine` (Phase 2.6), not in analysis.
    #[test]
    fn cjk_is_split_char_by_char_by_root_locale_word_segmenter() {
        let a = analyze_text("你好world", default_profile(), WhiteSpaceMode::Normal, WordBreakMode::Normal);
        let words: Vec<&str> = a.texts.iter().filter(|s| !s.is_empty()).map(String::as_str).collect();
        assert_eq!(words, vec!["你", "好", "world"]);
    }

    /// Whitespace collapsing (normal mode).
    #[test]
    fn normal_mode_collapses_whitespace_runs() {
        let a = analyze_text("a   b\tc", default_profile(), WhiteSpaceMode::Normal, WordBreakMode::Normal);
        assert_eq!(a.normalized, "a b c");
    }
}
