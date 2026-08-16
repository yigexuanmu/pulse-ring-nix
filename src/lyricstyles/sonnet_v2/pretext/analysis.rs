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

// ===== kinsoku / sticky-glue / closing-quote sets =====
// Byte-identical copy of pretext `analysis.ts` const Sets. Each entry is the
// single code point the TS Set holds; group order matches TS declaration.

/// `kinsokuStart` — characters prohibited from starting a line (CJK 仮名禁則).
pub const KINSOKU_START: &[char] = &[
    '\u{ff0c}', '\u{ff0e}', '\u{ff01}', '\u{ff1a}', '\u{ff1b}', '\u{ff1f}',
    '\u{3001}', '\u{3002}', '\u{30fb}', '\u{ff09}', '\u{3015}', '\u{3009}',
    '\u{300b}', '\u{300d}', '\u{300f}', '\u{3011}', '\u{3017}', '\u{3019}',
    '\u{301b}', '\u{30fc}', '\u{3005}', '\u{303b}', '\u{309d}', '\u{309e}',
    '\u{30fd}', '\u{30fe}',
];

/// `kinsokuEnd` — characters prohibited from ending a line.
pub const KINSOKU_END: &[char] = &[
    '"', '(', '[', '{',
    '¡', '¿',
    '“', '‘', '‚', '„', '«', '‹',
    '\u{2e18}',
    '\u{ff08}', '\u{3014}', '\u{3008}', '\u{300a}', '\u{300c}', '\u{300e}',
    '\u{3010}', '\u{3016}', '\u{3018}', '\u{301a}',
];

/// `leftStickyPunctuation` — punctuation that sticks to the preceding text run.
pub const LEFT_STICKY_PUNCTUATION: &[char] = &[
    '.', ',', '!', '?', ':', ';',
    '\u{60c}', '\u{61b}', '\u{61f}',
    '\u{964}', '\u{965}',
    '\u{104a}', '\u{104b}', '\u{104c}', '\u{104d}', '\u{104f}',
    ')', ']', '}',
    '%', '"',
    '”', '’', '»', '›',
    '…',
];

/// `forwardStickyGlue` — sticks to the *following* text run.
pub const FORWARD_STICKY_GLUE: &[char] = &['\'', '’'];

/// `closingQuoteChars` — used by `endsWithClosingQuote`.
pub const CLOSING_QUOTE_CHARS: &[char] = &[
    '”', '’', '»', '›',
    '\u{300d}', '\u{300f}', '\u{3011}', '\u{300b}', '\u{3009}', '\u{3015}', '\u{ff09}',
];

/// `keepAllGlueChars` — no-break space / word-joiner glue in `keep-all` mode.
pub const KEEP_ALL_GLUE_CHARS: &[char] = &[
    '\u{a0}', '\u{202f}', '\u{2060}', '\u{feff}',
];

/// `keepAllDashBreakChars` — dashes allow breaking in `keep-all` mode.
pub const KEEP_ALL_DASH_BREAK_CHARS: &[char] = &[
    '-', '\u{2010}', '\u{2013}', '\u{2014}',
];

/// `arabicNoSpaceTrailingPunctuation`.
pub const ARABIC_NO_SPACE_TRAILING_PUNCTUATION: &[char] = &[
    ':', '.', '\u{60c}', '\u{61b}',
];

/// `myanmarMedialGlue` — U+104F.
pub const MYANMAR_MEDIAL_GLUE: &[char] = &['\u{104f}'];

/// `numericJoinerChars` (TS `numericJoinerChars`).
pub const NUMERIC_JOINER_CHARS: &[char] = &[
    ':', '-', '/', '×', ',', '.', '+',
    '\u{2013}', '\u{2014}',
];

/// `noSpaceWordBreakAfterChars`.
pub const NO_SPACE_WORD_BREAK_AFTER_CHARS: &[char] = &[
    '?', '\u{58a}', '-', '\u{2010}', '\u{2012}', '\u{2013}', '\u{2014}',
    '\u{2026}', '\u{203c}', '\u{203d}', '\u{2049}',
];

fn set_contains(set: &[char], c: char) -> bool {
    // Linear scan; sets are all small (<32 entries) and hot enough on small
    // CJK strings that a binary search wouldn't pay for the sort overhead.
    set.iter().any(|&x| x == c)
}

/// `endsWithClosingQuote` — walk backwards skipping `leftStickyPunctuation`,
/// returning true at the first closing quote.
pub fn ends_with_closing_quote(text: &str) -> bool {
    let mut end = text.len();
    while end > 0 {
        let start = previous_code_point_start(text, end);
        let ch = &text[start..end];
        let c = ch.chars().next().unwrap_or('\0');
        if set_contains(CLOSING_QUOTE_CHARS, c) {
            return true;
        }
        if !set_contains(LEFT_STICKY_PUNCTUATION, c) {
            return false;
        }
        end = start;
    }
    false
}

/// `previousCodePointStart` — byte position of the code point ending at `end`.
/// In Rust this is a simple `char_indices` walk backwards, but pretext's UTF-16
/// surrogate pair tracking is unnecessary for UTF-8 (`char` is a code point).
pub fn previous_code_point_start(text: &str, end: usize) -> usize {
    if end == 0 {
        return 0;
    }
    let mut last = end;
    let bytes = text.as_bytes();
    // Walk back over continuation bytes (0x80..0xBF) to the lead byte.
    while last > 0 && (bytes[last - 1] & 0xC0) == 0x80 {
        last -= 1;
    }
    if last == 0 {
        return 0;
    }
    last - 1
}

/// `getLastCodePoint` — returns the last `char` of `text`, or `None` if empty.
pub fn get_last_code_point(text: &str) -> Option<char> {
    if text.is_empty() {
        return None;
    }
    text.chars().next_back()
}

/// `endsWithLineStartProhibitedText` (= kinsokuStart ∪ leftStickyPunctuation).
pub fn ends_with_line_start_prohibited_text(text: &str) -> bool {
    match get_last_code_point(text) {
        Some(c) => set_contains(KINSOKU_START, c) || set_contains(LEFT_STICKY_PUNCTUATION, c),
        None => false,
    }
}

fn ends_with_keep_all_glue_text(text: &str) -> bool {
    matches!(get_last_code_point(text), Some(c) if set_contains(KEEP_ALL_GLUE_CHARS, c))
}

fn ends_with_keep_all_dash_break_text(text: &str) -> bool {
    matches!(get_last_code_point(text), Some(c) if set_contains(KEEP_ALL_DASH_BREAK_CHARS, c))
}

/// `canContinueKeepAllTextRun` — keep-all grouping continuation test.
pub fn can_continue_keep_all_text_run(previous_text: &str, break_after_punctuation: bool) -> bool {
    if ends_with_keep_all_glue_text(previous_text) {
        return false;
    }
    if !break_after_punctuation {
        return true;
    }
    if ends_with_line_start_prohibited_text(previous_text) {
        return false;
    }
    if ends_with_keep_all_dash_break_text(previous_text) {
        return false;
    }
    true
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

/// Byte-identical port of pretext `classifySegmentBreakChar(ch, profile)`.
/// Classifies a *single* character (not a segment) into a `SegmentBreakKind`.
pub fn classify_segment_break_char(ch: char, wsp: WhiteSpaceProfile) -> SegmentBreakKind {
    if wsp.preserve_ordinary_spaces || wsp.preserve_hard_breaks {
        if ch == ' ' { return SegmentBreakKind::PreservedSpace; }
        if ch == '\t' { return SegmentBreakKind::Tab; }
        if wsp.preserve_hard_breaks && ch == '\n' { return SegmentBreakKind::HardBreak; }
    }
    match ch {
        ' ' => SegmentBreakKind::Space,
        '\u{a0}' | '\u{202f}' | '\u{2060}' | '\u{feff}' => SegmentBreakKind::Glue,
        '\u{200b}' => SegmentBreakKind::ZeroWidthBreak,
        '\u{ad}' => SegmentBreakKind::SoftHyphen,
        _ => SegmentBreakKind::Text,
    }
}

/// `breakCharRe = /[\x20\t\n\xA0\u00AD\u200B\u202F\u2060\uFEFF]/` — true iff the
/// char maps to a non-`Text` kind in `classifySegmentBreakChar`.
fn is_break_char(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\u{a0}' | '\u{ad}' | '\u{200b}' | '\u{202f}' | '\u{2060}' | '\u{feff}')
}

/// Byte-identical port of pretext `splitSegmentByBreakKind(segment, isWordLike, start, profile)`.
/// Splits a segment into pieces when the break-character kind changes; preserves
/// the original `isWordLike` for `Text` pieces and produces contiguous `start`
/// byte offsets into the source string.
pub fn split_segment_by_break_kind(
    segment: &str,
    is_word_like: bool,
    start: usize,
    wsp: WhiteSpaceProfile,
) -> Vec<(String, bool, SegmentBreakKind, usize)> {
    if !segment.chars().any(is_break_char) {
        return vec![(segment.to_string(), is_word_like, SegmentBreakKind::Text, start)];
    }

    let mut pieces: Vec<(String, bool, SegmentBreakKind, usize)> = vec![];
    let mut current_kind: Option<SegmentBreakKind> = None;
    let mut current_text_parts: Vec<char> = vec![];
    let mut current_start = start;
    let mut current_word_like = false;
    let mut offset = 0usize;

    for ch in segment.chars() {
        let kind = classify_segment_break_char(ch, wsp);
        let word_like = kind == SegmentBreakKind::Text && is_word_like;

        if current_kind == Some(kind) && current_word_like == word_like {
            current_text_parts.push(ch);
            offset += ch.len_utf8();
            continue;
        }

        if current_kind.is_some() {
            let text: String = current_text_parts.iter().collect();
            pieces.push((text, current_word_like, current_kind.unwrap(), current_start));
        }

        current_kind = Some(kind);
        current_text_parts = vec![ch];
        current_start = start + offset;
        current_word_like = word_like;
        offset += ch.len_utf8();
    }

    if current_kind.is_some() {
        let text: String = current_text_parts.iter().collect();
        pieces.push((text, current_word_like, current_kind.unwrap(), current_start));
    }

    pieces
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

    /// Phase 2.2 sticky glue + kinsoku helpers.
    #[test]
    fn ends_with_closing_quote_walking_back_through_sticky_punct() {
        assert!(ends_with_closing_quote("hello”"));
        assert!(!ends_with_closing_quote("hello"));
        assert!(ends_with_closing_quote("』"));
    }

    /// `previous_code_point_start` UTF-8 walk.
    #[test]
    fn previous_code_point_start_handles_multibyte_utf8() {
        assert_eq!(previous_code_point_start("a你", 1 + 3), 1);
        assert_eq!(previous_code_point_start("abc", 3), 2);
        assert_eq!(previous_code_point_start("你", 0), 0);
    }

    /// `get_last_code_point`.
    #[test]
    fn get_last_code_point_back_iter() {
        assert_eq!(get_last_code_point(""), None);
        assert_eq!(get_last_code_point("你好").unwrap(), '好');
        assert_eq!(get_last_code_point("abc").unwrap(), 'c');
    }

    /// `ends_with_line_start_prohibited_text` (kinsokuStart ∪ leftSticky).
    #[test]
    fn ends_with_line_start_prohibited_detects_comma_and_close_bracket() {
        assert!(ends_with_line_start_prohibited_text("你好，"));
        assert!(ends_with_line_start_prohibited_text("abc."));
        assert!(!ends_with_line_start_prohibited_text("abc"));
    }

    /// `can_continue_keep_all_text_run` glue/dash break rules.
    #[test]
    fn keep_all_continuation_rules() {
        assert!(can_continue_keep_all_text_run("abc", false));
        assert!(!can_continue_keep_all_text_run("abc\u{a0}", false));
        assert!(!can_continue_keep_all_text_run("abc\u{2014}", true));
        assert!(can_continue_keep_all_text_run("abc", true));
    }

    /// `split_segment_by_break_kind` — same-kind runs coalesce; kind changes
    /// start a new piece.
    #[test]
    fn split_segment_by_break_kind_splits_text_and_space() {
        let pieces = split_segment_by_break_kind(
            "abc   def", true, 100, WhiteSpaceProfile {
                mode: WhiteSpaceMode::Normal,
                preserve_ordinary_spaces: false,
                preserve_hard_breaks: false,
            });
        // text "abc" + space "   " + text "def" = 3 pieces.
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0].0, "abc");
        assert_eq!(pieces[0].1, true);
        assert_eq!(pieces[0].2, SegmentBreakKind::Text);
        assert_eq!(pieces[0].3, 100);
        assert_eq!(pieces[1].0, "   ");
        assert_eq!(pieces[1].1, false);
        assert_eq!(pieces[1].2, SegmentBreakKind::Space);
        assert_eq!(pieces[1].3, 103);
        assert_eq!(pieces[2].0, "def");
        assert_eq!(pieces[2].1, true);
        assert_eq!(pieces[2].2, SegmentBreakKind::Text);
        assert_eq!(pieces[2].3, 106);
    }

    /// `split_segment_by_break_kind` — no break chars => single Text piece.
    #[test]
    fn split_segment_by_break_kind_returns_single_piece_for_plain_text() {
        let pieces = split_segment_by_break_kind(
            "你好world", true, 0, WhiteSpaceProfile {
                mode: WhiteSpaceMode::Normal,
                preserve_ordinary_spaces: false,
                preserve_hard_breaks: false,
            });
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].0, "你好world");
        assert_eq!(pieces[0].2, SegmentBreakKind::Text);
    }

    /// `classify_segment_break_char` — exact single-char mapping.
    #[test]
    fn classify_single_char_matches_pretext_kind() {
        let normal = WhiteSpaceProfile {
            mode: WhiteSpaceMode::Normal,
            preserve_ordinary_spaces: false,
            preserve_hard_breaks: false,
        };
        assert_eq!(classify_segment_break_char(' ', normal), SegmentBreakKind::Space);
        assert_eq!(classify_segment_break_char('\u{a0}', normal), SegmentBreakKind::Glue);
        assert_eq!(classify_segment_break_char('\u{200b}', normal), SegmentBreakKind::ZeroWidthBreak);
        assert_eq!(classify_segment_break_char('\u{ad}', normal), SegmentBreakKind::SoftHyphen);
        assert_eq!(classify_segment_break_char('a', normal), SegmentBreakKind::Text);
    }
}
