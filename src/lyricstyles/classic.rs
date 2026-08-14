//! Classic style ("经典" / "classic" / "luminous" / "流动") — a faithful port of folia's
//! `classic` lyric visualizer.
//!
//! folia source: `src/components/visualizer/classic/{Visualizer.tsx, tuning.ts, entry.tsx}`.
//! `classic/Visualizer.tsx:10` — "the most straightforward lyric pipeline ... basically the
//! 'baseline' visualizer that the other modes keep borrowing timing ideas from." Unlike
//! sonnet it uses no Pixi/WebGL runtime, no MG decoration, no shot camera: it is a pure
//! React + framer-motion DOM lyric animation. The per-word motion is **lyric-timestamp
//! driven, not audio-coupled** — `audioPower`/`audioBands` only feed `VisualizerShell`'s
//! background reaction (`Visualizer.tsx:654`); the breathing float is keyed to
//! `theme.animationIntensity` + `breathingFloatMultiplier`, not audio (`:636-642`).
//!
//! **Translation to pulse-ring-nix's WGSL `lyric_words` channel** (no new pipeline, no shader
//! change — classic reuses the same `CharQuad`/`LyricFx` interface as sonnet):
//! - folia's two stacked spans (transparent-glow `text-shadow` layer + coloured body layer)
//!   collapse to **one glyph `CharQuad`** carrying both the RGBA `color` and a 0..1 `glow`
//!   halo (the wgsl glyph sampler zeros empty space, so the glow halo + body are one quad).
//! - folia's framer spring/tween state transitions → per-frame easing interpolation
//!   (`ease_in_out`/`ease_out`); `lyricContainerFloat` keyframes + `getClassicLineContainerMotion`
//!   enter/exit blur map to per-word position/alpha/scale and a frame `LyricFx.blur`
//!   (which draw.rs applies *only* to lyric quads, matching folia's body-layer blur).
//!
//! **Adaptations forced by pulse-ring-nix's model** (documented per-use):
//! - `DEFAULT_CLASSIC_TUNING` (`types.ts:392-396`) and `animationIntensity` default `'normal'`
//!   (`appearanceCodec.ts:32`) are baked in as Rust `const`s — pulse-ring-nix has no
//!   `ClassicTuning`/`animationIntensity` config field, so they are fixed at the folia defaults.
//! - pulse-ring-nix already groups consecutive CJK into one word (`lyricview::segment_words`).
//!   That is exactly `useLegacyLayout=false` semantic layout behaviour — that tuning branch
//!   is moot (see `types.ts:395`).
//! - `crate::lyrics::LyricLine` exposes no `isChorus`/`renderHints`/`wordColors`: the chorus
//!   ripple is wired but gated behind `is_chorus=false`, and word colours fall back to the
//!   shared palette (active→accent, passed→primary) — keyword colouring is a no-op until a
//!   chorus/word-colour adapter appears.

use crate::lyricview::{StyleCtx, StyleInput, StyleOutput};

// ============================================================================================
// folia classic tuning constants — ported byte-for-byte from the folia-major source tree.
// Every value carries its folia citation (`file:LINE`). `build_frame` consumes them in the
// follow-up commit; `#[allow(dead_code)]` is removed once the body lands.
// ============================================================================================
#[allow(dead_code)] // — kept until the build_frame body lands in the next commit.
mod tuning {
    // ---- DEFAULT_CLASSIC_TUNING (folia `src/types.ts:392-396`) ----
    /// `enableWordRotation` — `types.ts:393` (true).
    pub const ENABLE_WORD_ROTATION: bool = true;
    /// `breathingFloatMultiplier` — `types.ts:394` (1).
    pub const BREATHING_FLOAT_MULTIPLIER: f32 = 1.0;
    /// `useLegacyLayout` — `types.ts:395` (false). pulse-ring-nix's `segment_words` already does
    /// the non-legacy semantic grouping, so this branch never needs taking.
    pub const USE_LEGACY_LAYOUT: bool = false;
    /// `wordSpacing` — `types.ts:396` (0.7). Spacing multiplier on the computed inter-word gap.
    pub const WORD_SPACING: f32 = 0.7;

    // ---- tuning clamps (folia `classic/Visualizer.tsx:51-52`) ----
    /// `clampClassicBreathingFloatMultiplier` — `Math.min(2, Math.max(0, value))` (`:51`).
    pub const BFM_CLAMP_MAX: f32 = 2.0;
    /// `clampClassicWordSpacing` — `Math.min(2, Math.max(0, value))` (`:52`).
    pub const WS_CLAMP_MAX: f32 = 2.0;

    // ---- line timing-class thresholds (folia `utils/lyrics/renderHints.ts:11-13`) ----
    /// `MICRO_LINE_DURATION_THRESHOLD` (`renderHints.ts:11`).
    pub const MICRO_LINE_DURATION: f32 = 0.10;
    /// `SHORT_LINE_DURATION_THRESHOLD` (`renderHints.ts:12`).
    pub const SHORT_LINE_DURATION: f32 = 0.18;
    /// `MICRO_LINE_RENDER_FLOOR` (`renderHints.ts:13`).
    pub const MICRO_LINE_RENDER_FLOOR: f32 = 0.067;

    // ---- wordLookahead by reveal mode (folia `Visualizer.tsx:81`) ----
    pub const LOOKAHEAD_INSTANT: f32 = 0.03; // `:81`
    pub const LOOKAHEAD_FAST: f32 = 0.08; // `:81`
    pub const LOOKAHEAD_NORMAL: f32 = 0.15; // `:81`

    // ---- active-end / display-duration helpers (folia `Visualizer.tsx:84-105`) ----
    /// fast reveal: `min(lineRenderEnd, max(word.end, word.start + 0.12))` (`:91`).
    pub const ACTIVE_FAST_PAD: f32 = 0.12;
    pub const MIN_DURATION_INSTANT: f32 = 0.08; // `:100`
    pub const MIN_DURATION_FAST: f32 = 0.12; // `:100`
    pub const MIN_DURATION_NORMAL: f32 = 0.10; // `:101`

    // ---- line-container transition — `getClassicLineContainerMotion` (Visualizer.tsx:105-139) ----
    // 'normal': initial {opacity 0, scale 0.9, blur 10px} → animate {1, 1, 0}; exit {0, 1.1, blur 20px, 0.3s}
    pub const TRANS_NORMAL_ENTER_SCALE: f32 = 0.9; // `:137`
    pub const TRANS_NORMAL_ENTER_BLUR: f32 = 10.0; // `:137` (px → shader blur unit scaled in build_frame)
    pub const TRANS_NORMAL_EXIT_SCALE: f32 = 1.1; // `:139`
    pub const TRANS_NORMAL_EXIT_BLUR: f32 = 20.0; // `:139`
    pub const TRANS_NORMAL_EXIT_DUR: f32 = 0.30; // `:139`
    // 'fast': initial {opacity 0.35, scale 0.96, blur 4px} → animate {1, 1, 0, 0.16s}; exit {0, 1.04, blur 10px, 0.16s}
    pub const TRANS_FAST_ENTER_SCALE: f32 = 0.96; // `:119`
    pub const TRANS_FAST_ENTER_BLUR: f32 = 4.0; // `:119`
    pub const TRANS_FAST_EXIT_SCALE: f32 = 1.04; // `:128`
    pub const TRANS_FAST_EXIT_BLUR: f32 = 10.0; // `:130`
    pub const TRANS_FAST_DUR: f32 = 0.16; // `:124` / `:131`
    // 'none': static; exit {opacity 0, scale 1.02, blur 6px, 0.12s}
    pub const TRANS_NONE_EXIT_SCALE: f32 = 1.02; // `:113`
    pub const TRANS_NONE_EXIT_BLUR: f32 = 6.0; // `:113`
    pub const TRANS_NONE_EXIT_DUR: f32 = 0.12; // `:113`

    // ---- per-word layout RNG spread/rotate (folia `Visualizer.tsx:390-391`) ----
    /// `baseSpread` — chaotic 60 / calm 0 / normal 20 (`:390`).
    pub const BASE_SPREAD_CHAOTIC: f32 = 60.0;
    pub const BASE_SPREAD_CALM: f32 = 0.0;
    pub const BASE_SPREAD_NORMAL: f32 = 20.0;
    /// `baseRotate` — chaotic 30 / calm 0 / normal 5 (`:391`).
    pub const BASE_ROTATE_CHAOTIC: f32 = 30.0;
    pub const BASE_ROTATE_CALM: f32 = 0.0;
    pub const BASE_ROTATE_NORMAL: f32 = 5.0;

    // ---- per-word config scale range (folia `Visualizer.tsx:418`) ----
    /// non-chaotic: `1.1 + random(4) * 0.2` (`:418`).
    pub const WORD_SCALE_NORMAL_BASE: f32 = 1.1;
    pub const WORD_SCALE_NORMAL_RANGE: f32 = 0.2;
    /// chaotic: `0.8 + random(4) * 0.6` (`:418`).
    pub const WORD_SCALE_CHAOTIC_BASE: f32 = 0.8;
    pub const WORD_SCALE_CHAOTIC_RANGE: f32 = 0.6;
    /// active max scale multiplier — `config.scale * 1.4` (`:427`, `:494`).
    pub const ACTIVE_SCALE_MULT: f32 = 1.4;

    // ---- layoutVariants poses (folia `Visualizer.tsx:477-512`) ----
    pub const WORD_WAITING_OPACITY: f32 = 0.0; // `:479`
    pub const WORD_WAITING_SCALE: f32 = 0.5; // `:480`
    pub const WORD_ACTIVE_OPACITY: f32 = 1.0; // `:491`
    pub const WORD_PASSED_OPACITY_NORMAL: f32 = 0.82; // `:502` (chaotic? 0.9 : 0.82)
    pub const WORD_PASSED_OPACITY_CHAOTIC: f32 = 0.9; // `:502`
    pub const WORD_WAITING_DUR: f32 = 0.4; // `:481`
    pub const WORD_PASSED_DUR: f32 = 0.5; // `:503`
    /// passed rotate drift — linear over 5s (`:505`).
    pub const WORD_PASSED_ROTATE_DUR: f32 = 5.0;
    /// `passedRotate` span — `(random(8) - 0.5) * 45` (`:463`) → ±22.5°.
    pub const WORD_PASSED_ROTATE_RANGE: f32 = 45.0;

    // ---- bodyBlur (folia `Visualizer.tsx:516-540`) ----
    pub const BODY_BLUR_WAITING: f32 = 10.0; // `:517` 'blur(10px)'
    pub const BODY_BLUR_ACTIVE: f32 = 0.0; // `:525` 'none'
    pub const BODY_BLUR_PASSED: f32 = 0.0; // `:536` 'blur(0px)'

    // ---- glow text-shadow radii (folia `Visualizer.tsx:554-630`) ----
    // 'normal' multi-char: `0 0 20px color, 0 0 40px color` (`:600`).
    pub const GLOW_RADIUS_NORMAL: f32 = 20.0; // `:600`
    pub const GLOW_RADIUS_NORMAL_FAR: f32 = 40.0; // `:600`
    // 'fast': `0 0 18px, 0 0 32px` (`:573`).
    pub const GLOW_RADIUS_FAST: f32 = 18.0; // `:573`
    pub const GLOW_RADIUS_FAST_FAR: f32 = 32.0; // `:573`
    // 'instant': `0 0 14px, 0 0 24px` (`:557`).
    pub const GLOW_RADIUS_INSTANT: f32 = 14.0; // `:557`
    pub const GLOW_RADIUS_INSTANT_FAR: f32 = 24.0; // `:558`
    /// multi-char glow `times` peak — `[0, 0.3, 1]` (`:604`).
    pub const GLOW_TIMES_PEAK: f32 = 0.3; // `:604`
    /// single-char glow `times` peak — `[0, 0.9, 1]` (`:619`).
    pub const GLOW_SINGLE_TIMES_PEAK: f32 = 0.9; // `:619`
    /// glow fade stretch — `charDuration * 6` (`:604`).
    pub const GLOW_CHAR_DURATION_STRETCH: f32 = 6.0; // `:604`

    // ---- lyricContainerFloat — whole-line breathing (folia `Visualizer.tsx:636-642`) ----
    /// calm `{ distance: 10, duration: 8.5 }` (`:640`).
    pub const FLOAT_CALM_DISTANCE: f32 = 10.0;
    pub const FLOAT_CALM_DURATION: f32 = 8.5;
    /// normal `{ distance: 14, duration: 7 }` (`:641`).
    pub const FLOAT_NORMAL_DISTANCE: f32 = 14.0;
    pub const FLOAT_NORMAL_DURATION: f32 = 7.0;
    /// chaotic `{ distance: 18, duration: 5.8 }` (`:642`).
    pub const FLOAT_CHAOTIC_DISTANCE: f32 = 18.0;
    pub const FLOAT_CHAOTIC_DURATION: f32 = 5.8;
    /// scale keyframes — `[1, 1+0.01*mult, 1, 1-0.005*mult, 1]` (`:651`). With multiplier=1 → ±1%.
    pub const FLOAT_SCALE_SWING: f32 = 0.01; // `:651`
    pub const FLOAT_SCALE_DIP: f32 = 0.005; // `:651`
    // y keyframe waypoints — `[0, -dist, 0, +dist*0.45, 0]` (`:649`). Phase stops at 0/0.25/0.5/0.75/1.
    pub const FLOAT_Y_DOWN_FACTOR: f32 = 0.45; // `:649`

    // ---- chorus ripple (folia `Visualizer.tsx:178, 276-280`) ----
    /// `rippleScale = 1.5 + Math.random() * 2` (`:179`).
    pub const RIPPLE_SCALE_BASE: f32 = 1.5; // `:179`
    pub const RIPPLE_SCALE_RANGE: f32 = 2.0; // `:179`
    pub const RIPPLE_INITIAL_SCALE: f32 = 0.2; // `:274`
    pub const RIPPLE_INITIAL_OPACITY: f32 = 0.8; // `:274`
    pub const RIPPLE_DUR: f32 = 0.5; // `:278`

    // ---- font-size clamps (folia `Visualizer.tsx:343-346`, `lyricsFontScale` default 1) ----
    // main: `clamp(2.25rem, 6vw, 4.5rem)` = clamp(36px, 0.06*width, 72px) (`:343`)
    pub const FONT_MAIN_MIN_PX: f32 = 36.0; // 2.25rem (`:343`)
    pub const FONT_MAIN_VW: f32 = 0.06; // 6vw (`:343`)
    pub const FONT_MAIN_MAX_PX: f32 = 72.0; // 4.5rem (`:343`)
    // translation: `clamp(1.125rem, 2.6vw, 1.25rem)` = clamp(18px, 0.026*width, 20px) (`:345`)
    pub const FONT_SUB_MIN_PX: f32 = 18.0; // 1.125rem (`:345`)
    pub const FONT_SUB_VW: f32 = 0.026; // 2.6vw (`:345`)
    pub const FONT_SUB_MAX_PX: f32 = 20.0; // 1.25rem (`:345`)
}

/// Build one frame of the classic lyric animation.
///
/// Skeleton: the `LyricStyle::Classic` variant + dispatch in `lyricview::build_frame` are
/// wired, so `style = "classic"` builds standalone and never touches the sonnet engine. The
/// per-word state machine, per-grapheme glow sweep, layout RNG, breathing float, line
/// transition and translation subtitle are filled in by the next commit (`#3`). For now this
/// returns an empty frame so the wallpaper still drives the ring/particle/widget layers.
pub fn build_frame(_ctx: &StyleCtx, _input: &StyleInput) -> StyleOutput {
    StyleOutput::empty()
}
