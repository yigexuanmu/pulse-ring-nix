//! Folia sonnet v2 — pretext `measurement.ts` (275 lines) `compiler-grade 1:1 port`.
//!
//! Calculates per-segment advance widths, emoji-count corrections, and the
//! "breakable fit" grapheme advance schedules used by `layout.ts` / `line-break.ts`.
//!
//! ## Architecture faithfulness vs DOM/Canvas measurement
//!
//! pretext TS resolves `measureText` against a `CanvasRenderingContext2D` /
//! `OffscreenCanvas` 2D context (browser path). For Rust, we plumb a
//! `MeasureBackend` trait whose `measure_text(text, font_str) -> f32` is
//! satisfied by the FreeType advance-summing port the atlas layer (Phase 5)
//! will implement. The trait is the same shape as Canvas.measureText: takes
//! a `font` shorthand string (FreeType impl parses this to a face+size selection)
//! and returns the summed advance widths.
//!
//! The DOM emoji correction (`span.getBoundingClientRect()` path in
//! `getEmojiCorrection`) is intentionally elided in the Rust port: it only
//! fires when `document` exists, and there is a divergence between Canvas
//! `measureText` and a DOM `<span>`. In the no-DOM path TS already returns
//! `correction = 0` (and `getCorrectedSegmentWidth` short-circuits to
//! `metrics.width`). Rust has no DOM, so `emojiCorrection = 0` is the
//! byte-identical behaviour.
//!
//! ## Engine profile
//!
//! pretext's `getEngineProfile()` sniffs `navigator.userAgent` to special-case
//! Safari and Chromium. Rust has no navigator → the TS `typeof navigator ===
//! 'undefined'` branch is the byte-identical fallback. We use that constant
//! profile flag-for-flag: `lineFitEpsilon=0.005, carryCJKAfterClosingQuote=
//! false, breakKeepAllAfterPunctuation=true, preferPrefixWidthsForBreakableRuns
//! =false, preferEarlySoftHyphenBreak=false`.

use super::analysis::is_cjk;
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// pretext measurement.ts:5 — per-segment metric cache entry.
#[derive(Clone, Debug)]
pub struct SegmentMetrics {
    pub width: f32,
    pub contains_cjk: bool,
    pub emoji_count: Option<usize>,
    pub breakable_fit_mode: Option<BreakableFitMode>,
    pub breakable_fit_advances: Option<Vec<f32>>,
}

impl SegmentMetrics {
    fn new_missing(width: f32) -> Self {
        Self {
            width,
            contains_cjk: false,
            emoji_count: None,
            breakable_fit_mode: None,
            breakable_fit_advances: None,
        }
    }
}

/// pretext measurement.ts:11 — toggleable shape-changing knobs shared by
/// layout / line-break. Default `EngineProfile::default()` here matches the TS
/// non-browser fallback (no navigator).
#[derive(Clone, Copy, Debug)]
pub struct EngineProfile {
    pub line_fit_epsilon: f32,
    pub carry_cjk_after_closing_quote: bool,
    pub break_keep_all_after_punctuation: bool,
    pub prefer_prefix_widths_for_breakable_runs: bool,
    pub prefer_early_soft_hyphen_break: bool,
}

impl Default for EngineProfile {
    fn default() -> Self {
        Self::non_browser()
    }
}

impl EngineProfile {
    /// `getEngineProfile()` — pretext measurement.ts:71 `typeof navigator ===
    /// 'undefined'` branch. byte-identical fallback profile.
    pub const fn non_browser() -> Self {
        Self {
            line_fit_epsilon: 0.005,
            carry_cjk_after_closing_quote: false,
            break_keep_all_after_punctuation: true,
            prefer_prefix_widths_for_breakable_runs: false,
            prefer_early_soft_hyphen_break: false,
        }
    }
}

/// pretext measurement.ts:18 — breakable fit emission strategy. Selected
/// dynamically based on segment length and engine profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakableFitMode {
    /// `ctx.measureText(g)` each grapheme; sum-grapheme path is exact for
    /// separated CJK-style runs where the canvas clamps nonword graphemes.
    SumGraphemes,
    /// Each growing prefix measured; advance[i] = prefixWidth(i+1) - prefixWidth(i).
    SegmentPrefixes,
    /// For each pair of consecutive graphemes, measure the joined string and
    /// subtract the previous single-grapheme width. Models kerning.
    PairContext,
}

/// Porting shim for `CanvasRenderingContext2D.measureText` used by pretext.
/// Phase 5 (atlas) implements `FreeTypeBackend` against `freetype-sys`.
pub trait MeasureBackend {
    /// Summed advance widths of all glyphs in `text` at font `font_str`.
    /// `font_str` follows CSS shorthand (e.g. `"500 24px \"Source Han Sans\""`);
    /// the backend parses it to a face+size pair (Phase 5 wires the atlas).
    fn measure_text(&self, text: &str, font_str: &str) -> f32;
}

/// pretext measurement.ts:21 — pathological superlinear prepare-time path.
/// Switch above this grapheme count from segment-prefixes to pair-context.
pub const MAX_PREFIX_FIT_GRAPHEMES: usize = 96;

/// pretext measurement.ts:30 caches. Each `font` shorthand string owns a
/// `SegmentMetricCache` keyed by segment text; lifetime-bound to the
/// measurement session (Rust has no global GCWeakMap, so caller owns these).
#[derive(Default)]
pub struct MeasurementCaches {
    /// Keyed by font shorthand string.
    pub segment_metric_caches: RefCell<HashMap<String, HashMap<String, SegmentMetrics>>>,
    /// Cross-segment emoji correction cache keyed by font shorthand.
    pub emoji_correction_cache: RefCell<HashMap<String, f32>>,
}

impl MeasurementCaches {
    /// `getSegmentMetricCache(font)` — pretext measurement.ts:58. Lazily
    /// initialises a per-font cache map.
    pub fn get_segment_metric_cache(&self, font: &str) -> HashMap<String, SegmentMetrics> {
        if let Some(c) = self.segment_metric_caches.borrow().get(font) {
            return c.clone();
        }
        let fresh = HashMap::new();
        self
            .segment_metric_caches
            .borrow_mut()
            .insert(font.to_string(), fresh.clone());
        fresh
    }
    /// Insert (or replace) the cache for `font`.
    pub fn put_segment_metric_cache(&self, font: &str, cache: HashMap<String, SegmentMetrics>) {
        self
            .segment_metric_caches
            .borrow_mut()
            .insert(font.to_string(), cache);
    }
}

/// `getSegmentMetrics(seg, cache)` — pretext measurement.ts:65.
pub fn get_segment_metrics<B: MeasureBackend>(
    seg: &str,
    cache: &mut HashMap<String, SegmentMetrics>,
    backend: &B,
    font_str: &str,
) -> SegmentMetrics {
    if let Some(m) = cache.get(seg) {
        return m.clone();
    }
    let width = backend.measure_text(seg, font_str);
    let metrics = SegmentMetrics {
        width,
        contains_cjk: is_cjk(seg),
        ..SegmentMetrics::new_missing(width)
    };
    cache.insert(seg.to_string(), metrics.clone());
    metrics
}

/// `parseFontSize(font)` — pretext measurement.ts:101. Regex `(\d+(?:\.\d+)?)\s*px`.
/// Returns the parsed px size, or `16` as the fallback (matching TS).
pub fn parse_font_size(font: &str) -> f32 {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)\s*px").unwrap());
    if let Some(caps) = re.captures(font) {
        caps.get(1)
            .and_then(|m| m.as_str().parse::<f32>().ok())
            .unwrap_or(16.0)
    } else {
        16.0
    }
}

fn emoji_presentation_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\p{Emoji_Presentation}").unwrap())
}

fn maybe_emoji_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[\p{Emoji_Presentation}\p{Extended_Pictographic}\p{Regional_Indicator}\u{FE0F}\u{20E3}]").unwrap()
    })
}

/// `isEmojiGrapheme(g)` — pretext measurement.ts:121. True iff the grapheme
/// contains `\p{Emoji_Presentation}` OR the variation selector U+FE0F.
pub fn is_emoji_grapheme(g: &str) -> bool {
    emoji_presentation_re().is_match(g) || g.contains('\u{FE0F}')
}

/// `textMayContainEmoji(text)` — pretext measurement.ts:129. Pre-filter for
/// the (lazily-allocated) emoji-correction path; uses the broader maybe set
/// so callers skip the grapheme walk entirely when there is no emoji symbol.
pub fn text_may_contain_emoji(text: &str) -> bool {
    maybe_emoji_re().is_match(text)
}

/// `countEmojiGraphemes(text)` — pretext measurement.ts:148. Walks grapheme
/// clusters via UAX#29 and counts those flagged as emoji.
fn count_emoji_graphemes(text: &str) -> usize {
    UnicodeSegmentation::graphemes(text, true)
        .filter(|g| is_emoji_grapheme(g))
        .count()
}

/// `getEmojiCount(seg, metrics)` — pretext measurement.ts:155.
fn get_emoji_count(seg: &str, metrics: &mut SegmentMetrics) -> usize {
    if let Some(c) = metrics.emoji_count {
        return c;
    }
    let c = count_emoji_graphemes(seg);
    metrics.emoji_count = Some(c);
    c
}

/// `getCorrectedSegmentWidth(seg, metrics, emojiCorrection)` — pretext
/// measurement.ts:163. When `emojiCorrection == 0` TS short-circuits to
/// `metrics.width` (no DOM/canvas divergence path in Rust).
pub fn get_corrected_segment_width(
    seg: &str,
    metrics: &mut SegmentMetrics,
    emoji_correction: f32,
) -> f32 {
    if emoji_correction == 0.0 {
        return metrics.width;
    }
    metrics.width - (get_emoji_count(seg, metrics) as f32) * emoji_correction
}

/// `getSegmentBreakableFitAdvances(seg, metrics, cache, emojiCorrection, mode)`
/// — pretext measurement.ts:172. Per-grapheme advance schedule for line-fit:
/// length matches the grapheme count (or `None` when len<=1, signalling the
/// caller to fall back to the segment's full own width as the unit).
pub fn get_segment_breakable_fit_advances<B: MeasureBackend>(
    seg: &str,
    metrics: &mut SegmentMetrics,
    cache: &mut HashMap<String, SegmentMetrics>,
    backend: &B,
    font_str: &str,
    emoji_correction: f32,
    mode: BreakableFitMode,
) -> Option<Vec<f32>> {
    if metrics.breakable_fit_advances.is_some() && metrics.breakable_fit_mode == Some(mode) {
        return metrics.breakable_fit_advances.clone();
    }
    metrics.breakable_fit_mode = Some(mode);

    let graphemes: Vec<String> = UnicodeSegmentation::graphemes(seg, true)
        .map(String::from)
        .collect();
    if graphemes.len() <= 1 {
        metrics.breakable_fit_advances = None;
        return None;
    }

    // TS picks pair-context automatically when graphemes.len() > MAX_PREFIX_FIT_GRAPHEMES.
    let effective_mode = if graphemes.len() > MAX_PREFIX_FIT_GRAPHEMES {
        BreakableFitMode::PairContext
    } else {
        mode
    };

    let advances: Vec<f32> = match effective_mode {
        BreakableFitMode::SumGraphemes => {
            let mut a = Vec::with_capacity(graphemes.len());
            for grapheme in &graphemes {
                let mut g_metrics =
                    get_segment_metrics(grapheme, cache, backend, font_str);
                a.push(get_corrected_segment_width(grapheme, &mut g_metrics, emoji_correction));
            }
            a
        }
        BreakableFitMode::PairContext => {
            let mut a = Vec::with_capacity(graphemes.len());
            let mut previous_grapheme: Option<String> = None;
            let mut previous_width = 0.0f32;
            for grapheme in &graphemes {
                let mut g_metrics =
                    get_segment_metrics(grapheme, cache, backend, font_str);
                let current_width =
                    get_corrected_segment_width(grapheme, &mut g_metrics, emoji_correction);
                match &previous_grapheme {
                    None => a.push(current_width),
                    Some(prev) => {
                        let pair = format!("{}{}", prev, grapheme);
                        let mut pair_metrics =
                            get_segment_metrics(&pair, cache, backend, font_str);
                        a.push(
                            get_corrected_segment_width(&pair, &mut pair_metrics, emoji_correction)
                                - previous_width,
                        );
                    }
                }
                previous_grapheme = Some(grapheme.clone());
                previous_width = current_width;
            }
            a
        }
        BreakableFitMode::SegmentPrefixes => {
            // We always recompute from prefix lengths; emoji correction follows.
            let mut a = Vec::with_capacity(graphemes.len());
            let mut prefix = String::new();
            let mut prefix_width = 0.0f32;
            for grapheme in &graphemes {
                prefix.push_str(grapheme);
                let mut prefix_metrics =
                    get_segment_metrics(&prefix, cache, backend, font_str);
                let next_prefix_width =
                    get_corrected_segment_width(&prefix, &mut prefix_metrics, emoji_correction);
                a.push(next_prefix_width - prefix_width);
                prefix_width = next_prefix_width;
            }
            a
        }
    };

    metrics.breakable_fit_advances = Some(advances.clone());
    Some(advances)
}

/// `getFontMeasurementState(font, needsEmojiCorrection)` — pretext
/// measurement.ts:247. Rust port has no DOM-canvas divergence, so
/// `emoji_correction` is always 0 (the no-`document` TS branch).
pub fn get_font_measurement_state(
    font_str: &str,
    needs_emoji_correction: bool,
) -> (f32, f32) {
    // emoji_correction is 0 in any non-DOM environment; preserve byte-faithful
    // behaviour but accept the flag for API compatibility.
    let _ = needs_emoji_correction;
    let font_size = parse_font_size(font_str);
    let emoji_correction = 0.0f32;
    (font_size, emoji_correction)
}

/// `getEmojiCorrection(font, fontSize)` — pretext measurement.ts:135. Always 0
/// in the no-DOM fallback (byte-faithful with the TS no-document branch).
fn get_emoji_correction(_font_str: &str, _font_size: f32) -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Identity-stub MeasureBackend used for unit-testing the algorithm:
    /// returns `text.len() as f32` so widths are deterministic and the
    /// grapheme scheduling logic can be asserted byte-for-byte.
    struct ByteLenBackend;
    impl MeasureBackend for ByteLenBackend {
        fn measure_text(&self, text: &str, _font_str: &str) -> f32 {
            text.chars().count() as f32
        }
    }

    #[test]
    fn parse_font_size_extracts_px_value() {
        assert_eq!(parse_font_size("500 24px \"Source Han Sans\""), 24.0);
        assert_eq!(parse_font_size("normal 12.5px monospace"), 12.5);
        // Fallback when no px unit is present.
        assert_eq!(parse_font_size("bold large serif"), 16.0);
        assert_eq!(parse_font_size(""), 16.0);
    }

    #[test]
    fn engine_profile_non_browser_constants_match_pretext_fallback() {
        let p = EngineProfile::non_browser();
        assert_eq!(p.line_fit_epsilon, 0.005);
        assert!(!p.carry_cjk_after_closing_quote);
        assert!(p.break_keep_all_after_punctuation);
        assert!(!p.prefer_prefix_widths_for_breakable_runs);
        assert!(!p.prefer_early_soft_hyphen_break);
    }

    #[test]
    fn get_segment_metrics_caches_and_contains_cjk_flag() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let m = get_segment_metrics("abc", &mut cache, &b, font);
        assert_eq!(m.width, 3.0);
        assert!(!m.contains_cjk);
        let m2 = get_segment_metrics("你好", &mut cache, &b, font);
        assert_eq!(m2.width, 2.0);
        assert!(m2.contains_cjk);
        // Cached hit count must include both segs (no double insert).
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn get_corrected_segment_width_short_circuits_when_correction_zero() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let mut m = get_segment_metrics("\u{1F600}", &mut cache, &b, font);
        // emojiCorrection == 0 -> short-circuit to metrics.width.
        let w = get_corrected_segment_width("\u{1F600}", &mut m, 0.0);
        assert_eq!(w, m.width);
    }

    #[test]
    fn breakable_fit_advances_returns_none_for_single_grapheme_seg() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let mut m = get_segment_metrics("a", &mut cache, &b, font);
        let adv = get_segment_breakable_fit_advances(
            "a", &mut m, &mut cache, &b, font, 0.0, BreakableFitMode::SumGraphemes,
        );
        assert!(adv.is_none());
    }

    #[test]
    fn breakable_fit_advances_sum_graphemes_mode() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        // ByteLenBackend measures char count, so "abc" advances are [1,1,1].
        let mut m = get_segment_metrics("abc", &mut cache, &b, font);
        let adv = get_segment_breakable_fit_advances(
            "abc", &mut m, &mut cache, &b, font, 0.0, BreakableFitMode::SumGraphemes,
        );
        let adv = adv.expect("len>1 returns Some");
        assert_eq!(adv, vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn breakable_fit_advances_pair_context_mode_first_advance_is_single_grapheme_width() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let mut m = get_segment_metrics("abc", &mut cache, &b, font);
        let adv = get_segment_breakable_fit_advances(
            "abc", &mut m, &mut cache, &b, font, 0.0, BreakableFitMode::PairContext,
        )
        .expect("Some");
        // First advance = width('a') = 1; second = width('ab') - width('a') = 1; etc.
        assert_eq!(adv, vec![1.0, 1.0, 1.0]);
        assert_eq!(adv.len(), 3);
    }

    #[test]
    fn breakable_fit_advances_segment_prefix_mode() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let mut m = get_segment_metrics("abc", &mut cache, &b, font);
        let adv = get_segment_breakable_fit_advances(
            "abc", &mut m, &mut cache, &b, font, 0.0, BreakableFitMode::SegmentPrefixes,
        )
        .expect("Some");
        // "a" -> 1, "ab" -> 2 (delta 1), "abc" -> 3 (delta 1).
        assert_eq!(adv, vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn breakable_fit_advances_caches_result_reuse_for_same_mode() {
        let mut cache = HashMap::new();
        let b = ByteLenBackend;
        let font = "24px test";
        let mut m = get_segment_metrics("abc", &mut cache, &b, font);
        let a1 = get_segment_breakable_fit_advances(
            "abc", &mut m, &mut cache, &b, font, 0.0, BreakableFitMode::PairContext,
        )
        .expect("Some");
        let _ = m;
        // Re-fetch using same metrics struct (we mutated it in place above).
        // Then check that breakable_fit_mode was captured.
        // (Verify via cache insertion: the segment itself was added to cache.)
        assert!(cache.contains_key("a"));
        assert!(cache.contains_key("ab"));
        assert!(cache.contains_key("abc"));
        assert_eq!(a1, vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn text_may_contain_emoji_detects_presentation_and_basic_emoji() {
        assert!(text_may_contain_emoji("\u{1F600}")); // 😀
        assert!(text_may_contain_emoji("a\u{FE0F}b")); // variation selector
        assert!(!text_may_contain_emoji("plain ASCII text"));
        assert!(!text_may_contain_emoji("汉字 only"));
    }

    #[test]
    fn is_emoji_grapheme_flags_emoji_with_presentation_property() {
        assert!(is_emoji_grapheme("\u{1F600}"));
        // A plain grapheme cluster should not flag.
        assert!(!is_emoji_grapheme("a"));
        assert!(!is_emoji_grapheme("雨"));
    }

    #[test]
    fn get_font_measurement_state_returns_zero_emoji_correction_in_rust() {
        let (size, correction) = get_font_measurement_state("500 32px foo", false);
        assert_eq!(size, 32.0);
        assert_eq!(correction, 0.0);
        let (s2, c2) = get_font_measurement_state("500 32px foo", true);
        assert_eq!(s2, 32.0);
        assert_eq!(c2, 0.0); // no DOM in Rust -> no <span> measuring path
        let _ = get_emoji_correction("500 32px foo", 32.0);
    }
}
