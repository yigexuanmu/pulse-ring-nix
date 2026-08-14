//! Lyric animation styles, each a thin module over the shared core (`crate::lyricview`).
//!
//! To add a new style: create `<name>.rs` with `pub fn build_frame(ctx: &StyleCtx,
//! input: &StyleInput) -> Vec<CharQuad>`, add a `LyricStyle` variant in `config.rs`, one arm
//! in `lyricview::build_frame`, and a name alias in `config::parse_lyric_style`.

pub mod mg;
pub mod mg_geo;
pub mod mg_scene;
pub mod mg_themed;
pub mod sonnet;
pub mod staff_score;
