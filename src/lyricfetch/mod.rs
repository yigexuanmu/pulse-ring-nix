//! Native Rust lyric fetch layer — clean-room port of the bundled `lyric_sources.py`.
//!
//! Replaces the `python3 lyric_sources.py` subprocess (`lyrics.rs` used to spawn) with
//! in-process HTTP + format parsers. The public entry is [`fetch`], which dispatches by
//! [`TrackRequest::source`] to a chain of source adapters (LRCLIB / NetEase / QQ / Kugou /
//! SPlayer / QiShai / TTML / Spotify / Apple / Musixmatch), each producing a [`LyricData`]
//! directly — no JSON intermediate. Shared text/number helpers mirror the top-of-file
//! utilities in `lyric_sources.py` (cited per helper).

use crate::lyrics::{LyricData, TrackRequest};

pub(crate) mod lrc;

/// Fetch lyrics for `req`, dispatching by `req.source`.
///
/// Mirrors `lyric_sources.py::main` (lines 973-1018): lower-cases the source id, looks up the
/// adapter in the chain, and reports a clean `Err` (never panics) for unknown / unported
/// sources so the caller can fall back to the next source or report "no lyrics".
#[allow(dead_code)] // wired in a later commit
pub(crate) fn fetch(req: &TrackRequest) -> Result<LyricData, String> {
    let source = req.source.trim().to_ascii_lowercase();
    if req.title.trim().is_empty() && !matches!(source.as_str(), "" | "auto") {
        return Err(format!("lyricfetch: {source}: track title required"));
    }
    match source.as_str() {
        "" | "auto" => fetch_auto(req),
        // Source adapters are filled in incrementally; unported ids fall through cleanly.
        _ => Err(format!("lyricfetch: source '{source}' not yet ported")),
    }
}

fn fetch_auto(req: &TrackRequest) -> Result<LyricData, String> {
    Err(format!(
        "lyricfetch: auto chain not yet ported (track '{}')",
        req.title
    ))
}

// ---- shared helpers (mirror lyric_sources.py module utils, lines 28-60) ----

/// `clean_text` (lyric_sources.py:32): html-unescape, drop the BOM, then trim.
pub(crate) fn clean_text(value: &str) -> String {
    unescape_html(value).replace('\u{feff}', "").trim().to_string()
}

/// `number` (lyric_sources.py:38): `int(float(value))` with a fallback on malformed/inf input.
pub(crate) fn number(value: &str, default: i64) -> i64 {
    let s = value.trim();
    if s.is_empty() {
        return default;
    }
    match s.parse::<f64>() {
        Ok(f) if f.is_finite() => f as i64, // truncation toward zero matches int(float(...))
        _ => default,
    }
}

/// `normalize` (lyric_sources.py:45): lower-cased, alnum-only characters (used for fuzzy match).
#[allow(dead_code)] // used by source adapters as they land
pub(crate) fn normalize(value: &str) -> String {
    clean_text(value)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// `timestamp_ms` (lyric_sources.py:49): `[mm:ss(.fff)]` → ms; seconds may use `.` or `:`.
pub(crate) fn timestamp_ms(minutes: &str, seconds: &str) -> i64 {
    let secs = seconds.replace(':', ".");
    let m = number(minutes, 0) as f64;
    let s = secs.trim().parse::<f64>().unwrap_or(0.0);
    (m * 60000.0 + s * 1000.0).round() as i64
}

/// `duration_ms` (lyric_sources.py:54): treat >10e6 as microseconds, clap to 0 for <=0.
#[allow(dead_code)] // used by source adapters as they land
pub(crate) fn duration_ms(value: &str) -> i64 {
    let v = number(value, 0);
    if v <= 0 {
        0
    } else if v > 10_000_000 {
        v / 1000
    } else {
        v
    }
}

/// `finalize` (lyric_sources.py:131-166): keep lines with any visible text, sort timed-first,
/// then infer each line's duration from the next later line (or `total_duration`).
pub(crate) fn finalize(
    mut lines: Vec<crate::lyrics::LyricLine>,
    total_duration: i64,
) -> Vec<crate::lyrics::LyricLine> {
    lines.retain(|l| !l.text.is_empty() || !l.translation.is_empty() || !l.romanization.is_empty());
    lines.sort_by_key(|l| (l.start_ms < 0, if l.start_ms >= 0 { l.start_ms } else { 0 }));
    let n = lines.len();
    for i in 0..n {
        if lines[i].duration_ms > 0 || lines[i].start_ms < 0 {
            continue;
        }
        let start = lines[i].start_ms;
        let mut next_time = if total_duration > start { total_duration } else { 0 };
        for j in (i + 1)..n {
            if lines[j].start_ms > start {
                next_time = lines[j].start_ms;
                break;
            }
        }
        if next_time > 0 {
            lines[i].duration_ms = std::cmp::max(0, next_time - start);
        }
    }
    lines
}
// `merge_timed` (lyric_sources.py:168) ports with the NetEase source (later commit).

// ---- minimal html unescape (subset of python stdlib html.unescape) ----

use regex::Regex;
use std::sync::LazyLock;

static ENTITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&#(x[0-9a-fA-F]+|[0-9]+);|&([a-zA-Z][a-zA-Z0-9]+);").unwrap()
});

fn unescape_html(s: &str) -> String {
    ENTITY
        .replace_all(s, |c: &regex::Captures| {
            if let Some(num) = c.get(1) {
                let n = num.as_str();
                let cp = if let Some(h) = n.strip_prefix('x') {
                    u32::from_str_radix(h, 16)
                } else {
                    n.parse::<u32>()
                };
                return match cp.ok().and_then(char::from_u32) {
                    Some(ch) => ch.to_string(),
                    None => c[0].to_string(),
                };
            }
            let named = match c[2].to_ascii_lowercase().as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                "nbsp" => "\u{a0}",
                "hellip" => "…",
                "mdash" => "—",
                "ndash" => "–",
                "copy" => "©",
                "reg" => "®",
                "deg" => "°",
                "middot" => "·",
                "ldquo" => "“",
                "rdquo" => "”",
                "lsquo" => "‘",
                "rsquo" => "’",
                _ => return c[0].to_string(),
            };
            named.to_string()
        })
        .into_owned()
}
