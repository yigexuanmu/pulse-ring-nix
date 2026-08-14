//! Classic style ("经典") — a faithful port of folia's `classic` lyric visualizer.
//!
//! folia source is tiny: `src/components/visualizer/classic/{Visualizer.tsx, tuning.ts,
//! entry.tsx}` plus shared infra (`useVisualizerRuntime`, `renderHints`,
//! `cjkSemanticLayout`, `buildWordGraphemeTimings`). classic is folia's "baseline" lyric
//! pipeline (`classic/Visualizer.tsx:10`): one active line whose words fly through
//! `waiting -> active -> passed` while a per-grapheme glow sweep travels across each
//! active word, the whole line breathes, and a translation subtitle sits beneath. Unlike
//! sonnet it uses no Pixi/WebGL runtime, no MG decoration, no shot camera — it is
//! lyric-timestamp driven, not audio-driven (audio bands still power the shared wallpaper
//! ring/particles/widgets, which classic leaves to `draw.rs`).
//!
//! Translation to pulse-ring-nix's WGSL `lyric_words` channel (no new pipeline, no shader
//! change — classic reuses the same `CharQuad`/`LyricFx` interface):
//! - folia's two stacked spans (transparent-glow text-shadow layer + colored body layer)
//!   collapse to ONE glyph `CharQuad` carrying both the RGBA `color` and a 0..1 `glow` halo.
//!   The per-grapheme glow sweep → per-`CharQuad.glow` sampled at the frame time.
//! - framer-motion spring/tween word-state transitions → per-frame easing interpolation;
//!   `lyricContainerFloat` keyframes + `getClassicLineContainerMotion` enter/exit blur map to
//!   per-word position/alpha multipliers and a frame `LyricFx.blur`.
//! - folia `DEFAULT_CLASSIC_TUNING` (`types.ts:392-397`: enableWordRotation=true,
//!   breathingFloatMultiplier=1, useLegacyLayout=false, wordSpacing=0.7) and `animationIntensity`
//!   default `'normal'` (`utils/appearanceCodec.ts:43`) are baked in as Rust `const`s;
//!   pulse-ring-nix has no `ClassicTuning`/`animationIntensity` config field, so they are fixed
//!   at the folia defaults (documented per-use). pulse-ring-nix already groups consecutive CJK
//!   into one word (`lyricview::segment_words`), which is the `useLegacyLayout=false` semantic
//!   layout behaviour, so that tuning branch is moot.
//!
//! Adaptations forced by pulse-ring-nix's lyric model (`crate::lyrics::LyricLine` exposes no
//! `isChorus`/`renderHints`): `wordRevealMode`/`lineTransitionMode` default to `'normal'`
//! (folia `renderHints` fallback) and the chorus ripple is drawn but gated behind an
//! `is_chorus` flag that is always `false` until the adapter exposes chorus metadata.

use crate::lyricview::{StyleCtx, StyleInput, StyleOutput};

/// Build one frame of the classic lyric animation.
///
/// Skeleton: the `LyricStyle::Classic` variant + dispatch in `lyricview::build_frame` are
/// wired, so `style = "classic"` builds standalone and never touches the sonnet engine. The
/// per-word layout, per-grapheme glow sweep, line-container motion + breathing float, chorus
/// ripple and translation subtitle are implemented in follow-up commits. Returns an empty
/// frame for now.
pub fn build_frame(_ctx: &StyleCtx, _input: &StyleInput) -> StyleOutput {
    StyleOutput::empty()
}
