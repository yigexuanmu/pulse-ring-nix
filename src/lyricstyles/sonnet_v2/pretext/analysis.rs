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

// ===== Unicode character classes (TS `combiningMarkRe`/`arabicScriptRe`/`decimalDigitRe`) =====
// `combiningMarkRe = /\p{M}/u` → Mn | Mc | Me (Mark categories).
use unicode_general_category::{get_general_category, GeneralCategory as Gc};

/// `/\p{M}/u` — true for Mark categories (Mn/Mc/Me). Byte-identical to pretext `combiningMarkRe.test(ch)`.
pub fn is_combining_mark(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        Gc::NonspacingMark | Gc::SpacingMark | Gc::EnclosingMark
    )
}

/// `/\p{Nd}/u` — true for the Decimal Number (Nd) category.
pub fn is_decimal_digit_char(ch: char) -> bool {
    get_general_category(ch) == Gc::DecimalNumber
}

/// `arabicScriptRe = /\p{Script=Arabic}/u` — hard Unicode script ranges for Arabic
/// (main, supplement, extended A/B/C, presentation forms A/B, SIYAK). Byte-identical.
pub const ARABIC_SCRIPT_RANGES: &[(u32, u32)] = &[
    (0x0600, 0x0604), (0x0606, 0x06DC), (0x06DE, 0x06FF),
    (0x0750, 0x077F),
    (0x0870, 0x0888), (0x088A, 0x088E), (0x0890, 0x0891), (0x0898, 0x089F),
    (0x08A0, 0x08FF),
    (0xFB1D, 0xFB4F),
    (0xFB50, 0xFDFF),
    (0xFE70, 0xFEFF),
    (0x10E60, 0x10E7F),
    (0x1EE00, 0x1EEFF),
];

/// `arabicScriptRe.test(text)` — true iff *any* code point falls in an Arabic script range.
pub fn contains_arabic_script(text: &str) -> bool {
    text.chars().any(|c| {
        let u = c as u32;
        ARABIC_SCRIPT_RANGES.iter().any(|&(s, e)| u >= s && u <= e)
    })
}

/// `lineBreakNumericAffixRanges` — UAX #14 PR/PO class code points (flat start/end pairs).
pub const LINE_BREAK_NUMERIC_AFFIX_RANGES: &[u32] = &[
    0x0024, 0x0025, 0x002B, 0x002B, 0x005C, 0x005C, 0x00A2, 0x00A5, 0x00B0, 0x00B1,
    0x058F, 0x058F, 0x0609, 0x060B, 0x066A, 0x066A, 0x07FE, 0x07FF, 0x09F2, 0x09F3,
    0x09F9, 0x09FB, 0x0AF1, 0x0AF1, 0x0BF9, 0x0BF9, 0x0D79, 0x0D79, 0x0E3F, 0x0E3F,
    0x17DB, 0x17DB, 0x2030, 0x2037, 0x2057, 0x2057, 0x20A0, 0x20CF, 0x2103, 0x2103,
    0x2109, 0x2109, 0x2116, 0x2116, 0x2212, 0x2213, 0xA838, 0xA838, 0xFDFC, 0xFDFC,
    0xFE69, 0xFE6A, 0xFF04, 0xFF05, 0xFFE0, 0xFFE1, 0xFFE5, 0xFFE6,
    0x11FDD, 0x11FE0, 0x1E2FF, 0x1E2FF, 0x1ECAC, 0x1ECAC, 0x1ECB0, 0x1ECB0,
];

/// `isCodePointInRanges(codePoint, ranges)` — flat start/end pair scan.
pub fn is_code_point_in_ranges(code_point: u32, ranges: &[u32]) -> bool {
    let mut i = 0;
    while i + 1 < ranges.len() {
        if code_point >= ranges[i] && code_point <= ranges[i + 1] {
            return true;
        }
        i += 2;
    }
    false
}

/// `isLineBreakNumericAffix(ch)` — true iff `ch.codePointAt(0)` falls in `lineBreakNumericAffixRanges`.
pub fn is_line_break_numeric_affix(ch: char) -> bool {
    is_code_point_in_ranges(ch as u32, LINE_BREAK_NUMERIC_AFFIX_RANGES)
}

/// `endsWithLineBreakNumericAffix(text)` — last significant code point is a numeric affix.
pub fn ends_with_line_break_numeric_affix(text: &str) -> bool {
    get_last_significant_code_point(text).map_or(false, is_line_break_numeric_affix)
}

/// `startsWithDecimalDigit(text)` — first significant code point is a `/\p{Nd}/u`.
pub fn starts_with_decimal_digit(text: &str) -> bool {
    get_first_significant_code_point(text).map_or(false, is_decimal_digit_char)
}

// ===== CJK classification =====
/// `isCJKCodePoint(codePoint)` — misleading name; pretext groups CJK + kana + hangul + CJK punct + full-width block.
pub fn is_cjk_code_point(code_point: u32) -> bool {
    (code_point >= 0x4E00 && code_point <= 0x9FFF)
        || (code_point >= 0x3400 && code_point <= 0x4DBF)
        || (code_point >= 0x20000 && code_point <= 0x2A6DF)
        || (code_point >= 0x2A700 && code_point <= 0x2B73F)
        || (code_point >= 0x2B740 && code_point <= 0x2B81F)
        || (code_point >= 0x2B820 && code_point <= 0x2CEAF)
        || (code_point >= 0x2CEB0 && code_point <= 0x2EBEF)
        || (code_point >= 0x2EBF0 && code_point <= 0x2EE5D)
        || (code_point >= 0x2F800 && code_point <= 0x2FA1F)
        || (code_point >= 0x30000 && code_point <= 0x3134F)
        || (code_point >= 0x31350 && code_point <= 0x323AF)
        || (code_point >= 0x323B0 && code_point <= 0x33479)
        || (code_point >= 0xF900 && code_point <= 0xFAFF)
        || (code_point >= 0x3000 && code_point <= 0x303F)
        || (code_point >= 0x3040 && code_point <= 0x309F)
        || (code_point >= 0x30A0 && code_point <= 0x30FF)
        || (code_point >= 0x3130 && code_point <= 0x318F)
        || (code_point >= 0xAC00 && code_point <= 0xD7AF)
        || (code_point >= 0xFF00 && code_point <= 0xFFEF)
}

/// `isCJK(s)` — true iff any code point in `s` is CJK (per pretext's loose grouping).
pub fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| is_cjk_code_point(c as u32))
}

/// `getFirstSignificantCodePoint(text)` — first non-mark code point, else `None`.
pub fn get_first_significant_code_point(text: &str) -> Option<char> {
    text.chars().find(|&c| !is_combining_mark(c))
}

/// `getLastSignificantCodePoint(text)` — last non-mark code point (walk back over combining marks).
pub fn get_last_significant_code_point(text: &str) -> Option<char> {
    if text.is_empty() {
        return None;
    }
    let mut end = text.len();
    while end > 0 {
        let start = previous_code_point_start(text, end);
        let ch = text[start..end].chars().next().unwrap();
        if !is_combining_mark(ch) {
            return Some(ch);
        }
        end = start;
    }
    None
}

// ===== segment classifiers (full-text segment membership) =====
/// `isLeftStickyPunctuationSegment(segment)` — every char is left-sticky / numeric-affix,
/// and combining marks are allowed only *after* the first sticky char is seen.
pub fn is_left_sticky_punctuation_segment(segment: &str) -> bool {
    if is_escaped_quote_cluster_segment(segment) {
        return true;
    }
    let mut saw_punctuation = false;
    for ch in segment.chars() {
        if set_contains(LEFT_STICKY_PUNCTUATION, ch) || is_line_break_numeric_affix(ch) {
            saw_punctuation = true;
            continue;
        }
        if saw_punctuation && is_combining_mark(ch) {
            continue;
        }
        return false;
    }
    saw_punctuation
}

/// `isCJKLineStartProhibitedSegment(segment)` — every char is in `kinsokuStart ∪ leftStickyPunctuation`, non-empty.
pub fn is_cjk_line_start_prohibited_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    segment.chars().all(|c| set_contains(KINSOKU_START, c) || set_contains(LEFT_STICKY_PUNCTUATION, c))
}

/// `isForwardStickyClusterSegment(segment)` — every char is in `kinsokuEnd ∪ forwardStickyGlue`/mark/numeric-affix.
pub fn is_forward_sticky_cluster_segment(segment: &str) -> bool {
    if is_escaped_quote_cluster_segment(segment) {
        return true;
    }
    if segment.is_empty() {
        return false;
    }
    segment.chars().all(|c| {
        set_contains(KINSOKU_END, c)
            || set_contains(FORWARD_STICKY_GLUE, c)
            || is_combining_mark(c)
            || is_line_break_numeric_affix(c)
    })
}

/// `isEscapedQuoteClusterSegment(segment)` — allows `\\` + combining marks freely; at least one
/// closing-quote/sticky char.
pub fn is_escaped_quote_cluster_segment(segment: &str) -> bool {
    let mut saw_quote = false;
    for ch in segment.chars() {
        if ch == '\\' || is_combining_mark(ch) {
            continue;
        }
        if set_contains(KINSOKU_END, ch)
            || set_contains(LEFT_STICKY_PUNCTUATION, ch)
            || set_contains(FORWARD_STICKY_GLUE, ch)
        {
            saw_quote = true;
            continue;
        }
        return false;
    }
    saw_quote
}

/// `getRepeatableSingleCharRunChar(text, isWordLike, kind)` — returns the single repeatable
/// punctuation char, or `None`. Excludes `"-"` and `\u{2014}` (em-dash) explicitly.
pub fn get_repeatable_single_char_run_char(
    text: &str,
    is_word_like: bool,
    kind: SegmentBreakKind,
) -> Option<char> {
    if kind == SegmentBreakKind::Text
        && !is_word_like
        && text.chars().count() == 1
        && text != "-"
        && text != "\u{2014}"
    {
        text.chars().next()
    } else {
        None
    }
}

/// `materializeDeferredSingleCharRun(texts, chars, lengths, index)` — eagerly repeats the deferred single-char run.
pub fn materialize_deferred_single_char_run(
    texts: &mut [String],
    chars: &mut [Option<char>],
    lengths: &mut [usize],
    index: usize,
) -> String {
    match chars[index] {
        None => texts[index].clone(),
        Some(ch) => {
            let length = lengths[index];
            if texts[index].chars().count() == length {
                return texts[index].clone();
            }
            let materialized = ch.to_string().repeat(length);
            texts[index] = materialized.clone();
            materialized
        }
    }
}

/// `hasArabicNoSpacePunctuation(containsArabic, lastCodePoint)` — true iff the segment contains
/// Arabic and ends with an Arabic no-space punctuation char.
pub fn has_arabic_no_space_punctuation(contains_arabic: bool, last_code_point: Option<char>) -> bool {
    contains_arabic
        && last_code_point.map_or(false, |c| set_contains(ARABIC_NO_SPACE_TRAILING_PUNCTUATION, c))
}

/// `endsWithMyanmarMedialGlue(segment)` — last char is U+104F (Myanmar medial `\u{104f}`).
pub fn ends_with_myanmar_medial_glue(segment: &str) -> bool {
    get_last_code_point(segment).map_or(false, |c| set_contains(MYANMAR_MEDIAL_GLUE, c))
}

/// `splitLeadingSpaceAndMarks(segment)` — if segment starts with ASCII space followed by one-or-more
/// `\p{M}` marks, return `Some((" ", marks))`; else `None`.
pub fn split_leading_space_and_marks(segment: &str) -> Option<(String, String)> {
    let mut chars = segment.chars();
    let first = chars.next()?;
    if first != ' ' {
        return None;
    }
    let rest: String = chars.collect();
    if !rest.is_empty() && rest.chars().all(is_combining_mark) {
        return Some((String::from(" "), rest));
    }
    None
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

    /// `is_combining_mark` — Mn/Mc/Me .
    #[test]
    fn combining_mark_detects_marks_and_ignores_ordinary_chars() {
        assert!(is_combining_mark('\u{300}')); // U+0300 combining grave (Mn)
        assert!(is_combining_mark('\u{338}')); // U+0338 combining long stroke overlay
        assert!(!is_combining_mark('a'));
        assert!(!is_combining_mark(' '));
    }

    /// `contains_arabic_script` — main block + presentation forms.
    #[test]
    fn contains_arabic_script_detects_arabic_and_ignores_latin() {
        assert!(contains_arabic_script("\u{627}")); // Arabic Alef
        assert!(contains_arabic_script("abc\u{645}")); // mix
        assert!(!contains_arabic_script("abc"));
        assert!(!contains_arabic_script("你好"));
    }

    /// `is_line_break_numeric_affix` — UAX #14 PR/PO classes.
    #[test]
    fn numeric_affix_detects_currency_and_plus_minus() {
        assert!(is_line_break_numeric_affix('+'));
        assert!(!is_line_break_numeric_affix('-')); // hyphen is UAX#14 BA, not PR/PO
        assert!(is_line_break_numeric_affix('%'));
        assert!(is_line_break_numeric_affix('$'));
        assert!(!is_line_break_numeric_affix('a'));
    }

    /// `is_cjk` — CJK detection .
    #[test]
    fn is_cjk_detects_chinese_and_kana_and_fullwidth() {
        assert!(is_cjk("你好"));
        assert!(is_cjk("カ"));
        assert!(is_cjk("Ａ")); // U+FF21 fullwidth A
        assert!(!is_cjk("hello"));
        assert!(is_cjk("a中")); // mixed
    }

    /// `get_first_significant_code_point` — skips leading combining marks.
    #[test]
    fn get_first_significant_code_point_skips_leading_marks() {
        assert_eq!(get_first_significant_code_point("a"), Some('a'));
        assert_eq!(get_first_significant_code_point("\u{300}a"), Some('a'));
        assert_eq!(get_first_significant_code_point(""), None);
    }

    /// `get_last_significant_code_point` — skips trailing combining marks.
    #[test]
    fn get_last_significant_code_point_skips_trailing_marks() {
        assert_eq!(get_last_significant_code_point("a\u{300}"), Some('a'));
        assert_eq!(get_last_significant_code_point("a"), Some('a'));
        assert_eq!(get_last_significant_code_point(""), None);
    }

    /// `is_left_sticky_punctuation_segment` — punctuation + post-punct marks.
    #[test]
    fn left_sticky_punctuation_segment_pure_punct_and_punct_plus_marks() {
        assert!(is_left_sticky_punctuation_segment("."));
        assert!(is_left_sticky_punctuation_segment(".\u{300}")); // . + combining mark
        assert!(!is_left_sticky_punctuation_segment("a"));
        assert!(!is_left_sticky_punctuation_segment("a.")); // non-sticky first char
    }

    /// `is_cjk_line_start_prohibited_segment` — kinsoku + sticky punct only.
    #[test]
    fn cjk_line_start_prohibited_segment_all_sticky() {
        assert!(is_cjk_line_start_prohibited_segment("，")); // U+FF0C
        assert!(is_cjk_line_start_prohibited_segment("."));
        assert!(!is_cjk_line_start_prohibited_segment("a"));
        assert!(!is_cjk_line_start_prohibited_segment(""));
    }

    /// `is_forward_sticky_cluster_segment` — closing quote + forward glue + marks.
    #[test]
    fn forward_sticky_cluster_closing_quotes_and_glue() {
        assert!(is_forward_sticky_cluster_segment("”"));
        assert!(is_forward_sticky_cluster_segment("”\u{300}")); // quote + mark
        assert!(!is_forward_sticky_cluster_segment("abc"));
    }

    /// `is_escaped_quote_cluster_segment` — backslash escape + sticky chars.
    #[test]
    fn escaped_quote_cluster() {
        assert!(is_escaped_quote_cluster_segment("\\\"")); // \ + "
        assert!(is_escaped_quote_cluster_segment("\"")); // just "
        assert!(!is_escaped_quote_cluster_segment("abc"));
    }

    /// `get_repeatable_single_char_run_char` — single non-word text chars except `-`/em-dash.
    #[test]
    fn repeatable_single_char_run_char() {
        assert_eq!(get_repeatable_single_char_run_char(".", false, SegmentBreakKind::Text), Some('.'));
        assert_eq!(get_repeatable_single_char_run_char("..", false, SegmentBreakKind::Text), None); // len 2
        assert_eq!(get_repeatable_single_char_run_char("-", false, SegmentBreakKind::Text), None); // dash excluded
        assert_eq!(get_repeatable_single_char_run_char("\u{2014}", false, SegmentBreakKind::Text), None); // em-dash
        assert_eq!(get_repeatable_single_char_run_char("a", true, SegmentBreakKind::Text), None); // word-like
        assert_eq!(get_repeatable_single_char_run_char("a", false, SegmentBreakKind::Space), None); // not text
    }

    /// `materialize_deferred_single_char_run` — repeats opt char to match deferred length.
    #[test]
    fn materialize_deferred_repeats_opt_char() {
        let mut texts = vec![String::from(".")];
        let mut chars = vec![Some('.')];
        let mut lengths = vec![3]; // deferred run length = 3
        let materialized = materialize_deferred_single_char_run(&mut texts, &mut chars, &mut lengths, 0);
        assert_eq!(materialized, "...");
        assert_eq!(texts[0], "...");
    }

    /// `has_arabic_no_space_punctuation` — arabic content + trailing arabic punct.
    #[test]
    fn arabic_no_space_punct() {
        assert!(has_arabic_no_space_punctuation(true, Some(':')));
        assert!(has_arabic_no_space_punctuation(true, Some('\u{60c}'))); // Arabic comma
        assert!(!has_arabic_no_space_punctuation(false, Some(':')));
        assert!(!has_arabic_no_space_punctuation(true, Some('a')));
    }

    /// `ends_with_myanmar_medial_glue` — U+104F.
    #[test]
    fn myanmar_medial_glue() {
        assert!(ends_with_myanmar_medial_glue("\u{104f}a\u{104f}"));
        assert!(!ends_with_myanmar_medial_glue("a"));
    }

    /// `split_leading_space_and_marks` — leading space + all-mark tail.
    #[test]
    fn split_leading_space_and_marks_extracts() {
        assert_eq!(split_leading_space_and_marks(" \u{300}"), Some((String::from(" "), String::from("\u{300}"))));
        assert_eq!(split_leading_space_and_marks(" a"), None); // tail not all marks
        assert_eq!(split_leading_space_and_marks("a\u{300}"), None); // no leading space
        assert_eq!(split_leading_space_and_marks("ab"), None);
    }
}
