//! Folia sonnet v2 lyric utilities — `utils/lyrics/graphemeTiming.ts` (154 lines) +
//! `utils/lyrics/renderHints.ts` (243 lines) compiler-grade 1:1 port.
//!
//! Both files are pure pure-function helpers consumed by `sonnetSemantic.ts` and
//! `sonnetProgram.ts`. They live outside the sonnet directory in folia (because all
//! visualizers share them), but in this port they are kept under `sonnet_v2/lyrics_util/`
//! so the v2 module is self-contained without touching the legacy sonnet path.

pub mod grapheme_timing;
pub mod render_hints;
