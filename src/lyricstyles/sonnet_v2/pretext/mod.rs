//! Pretext pure-text-algorithm port (Phase 2 of the sonnet v2 plan).
//!
//! Byte-identical 1:1 Rust port of `@chenglou/pretext` v0.0.8 — the text shaping
//! library chenglou ships with PixiJS. folia's `sonnetTypographyLayout.ts`
//! delegates *all* measurement / breaking / layout work here, so this module is
//! a load-bearing foundational layer for the typography port in Phase 4.
//!
//! File-by-file map (source `src/` line counts):
//!   2.1 `bidi_data`      (996 → 808)  generated bidi class tables
//!   2.2 `bidi`           (175)         simplified UAX#9 level resolver
//!   2.3 `analysis`       (1458)        Intl.Segmenter word/CJK grapheme
//!   2.4 `line_text`      (107)         PreparedText/Segment data model
//!   2.5 `measurement`    (275)         FreeType advance (replaces Canvas)
//!   2.6 `line_break`     (1236)        line breaking
//!   2.7 `layout`         (914)         line layout / justification
//!   2.8 `rich_inline`    (518)         inline runs
//!
//! See `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` Phase 2.

pub mod analysis;
pub mod bidi;
pub mod bidi_data;

/// Placeholder for the shared `Intl.Segmenter('word')` (Phase 2.6 layout.rs
/// takes ownership of it via analysis.rs). Removed once `analysis.rs` builds
/// its own segmenter struct.
pub struct SharedWordSegmenter;
