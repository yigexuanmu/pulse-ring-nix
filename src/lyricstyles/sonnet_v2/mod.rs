//! Folia sonnet engine, compiler-grade 1:1 port from TypeScript.
//!
//! See `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` for the full plan
//! (55 folia TS/TSX files ≈ 10,853 lines + `@chenglou/pretext` ≈ 5,279 lines → Rust).
//!
//! # Architecture (decision X)
//!
//! Persistent scene-graph arena + integer IDs (`SonnetSceneArena{scenes, shots,
//! segments, glyphs, ghosts, mg_layers, guides, frame_decors}`). `render_frame(t)`
//! literally mutates arena fields — a direct mirror of PixiJS `view.alpha = ...` /
//! `view.position.set(...)`. Frame end, `flatten` produces `Vec<CharQuad>` fed to
//! the existing `draw.rs` / WGSL `scene_at` (zero change for the non-glyph layer).
//!
//! Glyph coverage atlas is FreeType (G1 plan): replaces the `fontdue` SDF path so
//! the rendered pixels are byte-identical to a FreeType reference raster.
//!
//! # Wiring
//!
//! The dispatch in `crate::lyricview::build_frame` keeps routing to the legacy
//! `crate::lyricstyles::sonnet::build_frame` until Phase 9.3. This module compiles
//! from Phase 0.2 onward but does not participate in the live frame loop until
//! every Phase is green and snapshotted.

pub mod camera_tracking;
pub mod font_stack;
pub mod lyrics_util;
pub mod motion;
pub mod pretext;
pub mod program;
pub mod random;
pub mod semantic;
pub mod shot_flow_layouts;
pub mod transitions;
pub mod typography_roles;
pub mod types;

/// Placeholder entry — identical signature to the legacy
/// `crate::lyricstyles::sonnet::build_frame`.
///
/// Returns an empty `StyleOutput` while Phases 1–9 fill out the module. The legacy
/// sonnet stays the real dispatch, so this stub is never invoked on the live frame
/// loop until the Phase 9.3 switch.
pub fn build_frame(
    _ctx: &crate::lyricview::StyleCtx,
    _input: &crate::lyricview::StyleInput,
) -> crate::lyricview::StyleOutput {
    crate::lyricview::StyleOutput::empty()
}
