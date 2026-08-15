//! TTML adapter — shipping scaffold.
//!
//! This module lands in two commits: first a minimal stub so the shared
//! `jsonparse::parse_json_lines` / `jsonparse::parse_payload` TTML string route resolves
//! cleanly; then the full parser + `fetch_ttml` + `ttml` dispatch arm.

use crate::lyrics::LyricLine;

/// TTML body parser placeholder — returns no lines until the real parser lands.
pub(crate) fn parse_ttml(_text: &str) -> Vec<LyricLine> {
    Vec::new()
}
