//! LRCLIB source adapter — clean-room port of `lyric_sources.py::adapter_lrclib` (lines
//! 605-626). LRCLIB's `/api/search` returns a JSON array of `{id, trackName, artistName,
//! albumName, duration, syncedLyrics, plainLyrics}` candidates. We rank them via
//! [`lrclib_candidates`] (464-530), take the highest-ranked match, and parse its
//! `syncedLyrics` (preferring synced over plain). Lines are `finalize`-d against the track
//! duration — mirroring Python's `success(..., duration, ...)` double-finalize (parse_lrc
//! already finalizes with 0, then `success` re-finalizes with the real duration).
//!
//! Parity scope: `itunes_cover` (567) and the `candidates` / `selected_candidate_id`
//! response fields have no home in [`crate::lyrics::LyricData`] (it carries only `source`
//! + `lines`), so they are intentionally not ported here; track cover is a separate concern.

use crate::lyrics::{LyricData, TrackRequest};
use serde_json::Value;
use super::{clean_text, finalize, normalize, number, query_url, request_json, REQUEST_TIMEOUT};
use super::lrc::{parse_lrc, parse_plain};

/// Adapter entry: search LRCLIB and return parsed lyrics for `req`.
/// Mirrors `adapter_lrclib` (lyric_sources.py:605-626).
pub(crate) fn fetch_lrclib(req: &TrackRequest) -> Result<LyricData, String> {
    // Build the /api/search query (lyric_sources.py:607-609): track + artist always,
    // album only when present.
    let mut params: Vec<(&str, String)> = Vec::with_capacity(3);
    params.push(("track_name", req.title.clone()));
    params.push(("artist_name", req.artist.clone()));
    if !req.album.trim().is_empty() {
        params.push(("album_name", req.album.clone()));
    }
    let url = query_url("https://lrclib.net/api/search", &params); // :610
    let data = request_json(&url, REQUEST_TIMEOUT).map_err(|e| format!("lrclib: {e}"))?;
    parse_lrclib_response(&data, req)
}

/// HTTP-free selection + parse — the testable core of `adapter_lrclib`.
fn parse_lrclib_response(data: &Value, req: &TrackRequest) -> Result<LyricData, String> {
    let items: Vec<&Value> = data
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let matches = lrclib_candidates(&items, req);
    // `options.lyrics_candidate_id` (613) has no surface in TrackRequest, so `best` is the
    // highest-ranked match — matching Python's default auto-selection.
    let best = match matches.first() {
        Some(item) => *item,
        None => return Err("lrclib: no match".to_string()), // :619
    };
    let lyrics = best
        .get("syncedLyrics")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| best.get("plainLyrics").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string(); // :621 (syncedLyrics || plainLyrics || "")
    // Native `req.duration_ms` is already ms; Python reaches the same value through
    // `duration_ms(track.duration)` (which unmangles the ms*1000 the subprocess path sent).
    let total = req.duration_ms;
    let parsed = parse_lrc(&lyrics);
    let parsed = if !parsed.is_empty() { parsed } else { parse_plain(&lyrics) }; // :622
    let lines = finalize(parsed, total); // success(...) -> finalize(lines, total)
    if lines.is_empty() {
        return Err("lrclib: no lyrics".to_string()); // success([]) -> empty(...)
    }
    Ok(LyricData { source: "lrclib".to_string(), lines })
}

/// `lrclib_candidates` (lyric_sources.py:464-530): rank search hits by the key
/// `(−identity, duration_bucket, −synced, duration_diff, index)` and return the items in
/// that order. Identity requires title/artist matches (a zero score skips the candidate).
fn lrclib_candidates<'a>(items: &'a [&Value], req: &TrackRequest) -> Vec<&'a Value> {
    // (lyric_sources.py:465-468)
    let wanted_title = normalize(&req.title);
    let wanted_artist = normalize(&req.artist);
    let wanted_album = normalize(&req.album);
    let wanted_duration = req.duration_ms as f64 / 1000.0; // seconds

    // key: (−identity, bucket, −synced, diff, index) plus the borrowed item.
    let (
        mut ranked,
        mut seen_ids,
    ): (
        Vec<(i64, i64, i64, f64, usize, &'a Value)>,
        std::collections::HashSet<String>,
    ) = (Vec::new(), std::collections::HashSet::new());

    for (index, item) in items.iter().enumerate() {
        let obj = match item.as_object() {
            Some(o) => o,
            None => continue,
        };
        // (lyric_sources.py:473-478)
        let id = clean_text(&str_field(obj, "id"));
        if id.is_empty() || seen_ids.contains(&id) {
            continue;
        }
        let synced = !clean_text(&str_field(obj, "syncedLyrics")).is_empty();
        let plain = !clean_text(&str_field(obj, "plainLyrics")).is_empty();
        if !synced && !plain {
            continue;
        }

        let title = normalize(&str_field(obj, "trackName"));
        let artist = normalize(&str_field(obj, "artistName"));
        let album = normalize(&str_field(obj, "albumName"));

        // title required; exact=6 / substr=3; a zero skips (lyric_sources.py:486-497)
        if !wanted_title.is_empty() && title.is_empty() {
            continue;
        }
        let mut identity_score: i64 = 0;
        if !wanted_title.is_empty() && !title.is_empty() {
            let title_score = if title == wanted_title {
                6
            } else if wanted_title.contains(&title) || title.contains(&wanted_title) {
                3
            } else {
                0
            };
            if title_score == 0 {
                continue;
            }
            identity_score += title_score;
        }
        // artist: exact=4 / substr=2; zero skips (lyric_sources.py:498-510)
        if !wanted_artist.is_empty() && !artist.is_empty() {
            let artist_score = if artist == wanted_artist {
                4
            } else if wanted_artist.contains(&artist) || artist.contains(&wanted_artist) {
                2
            } else {
                0
            };
            if artist_score == 0 {
                continue;
            }
            identity_score += artist_score;
        }
        // album: exact=2 / substr=1, no skip on zero (lyric_sources.py:511-513)
        if !wanted_album.is_empty() && !album.is_empty() {
            identity_score += if album == wanted_album {
                2
            } else if wanted_album.contains(&album) || album.contains(&wanted_album) {
                1
            } else {
                0
            };
        }

        // duration bucket (lyric_sources.py:515-528)
        let candidate_duration = jnum(obj.get("duration"), 0);
        let mut duration_bucket: i64 = 5;
        let mut duration_difference: f64 = f64::INFINITY;
        if wanted_duration > 0.0 && candidate_duration as f64 > 0.0 {
            duration_difference = (wanted_duration - candidate_duration as f64).abs();
            duration_bucket = if duration_difference <= 2.0 {
                0
            } else if duration_difference <= 5.0 {
                1
            } else if duration_difference <= 10.0 {
                2
            } else if duration_difference <= 20.0 {
                3
            } else {
                4
            };
        }

        if identity_score >= 3 {
            // (lyric_sources.py:530) collect for `ranked.sort(key=lambda e: e[:-1])`
            seen_ids.insert(id);
            ranked.push((
                -identity_score,
                duration_bucket,
                -(synced as i64),
                duration_difference,
                index,
                item,
            ));
        }
    }

    // ascending sort by the 5-tuple key (lyric_sources.py:532).
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.4.cmp(&b.4))
    });
    ranked.into_iter().map(|e| e.5).collect()
}

/// Read an object field as a cleaned string, tolerating numbers/bools — matches Python's
/// `item.get(...)` + `clean_text` over whatever JSON flavour LRCLIB returns.
fn str_field(obj: &serde_json::Map<String, Value>, key: &str) -> String {
    match obj.get(key) {
        Some(Value::String(s)) => clean_text(s),
        Some(v) => clean_text(&v.to_string()),
        None => String::new(),
    }
}

/// Python `number(value, default)` over a `serde_json::Value`: a JSON number (int/float) or
/// a numeric string; otherwise `default`.
fn jnum(v: Option<&Value>, default: i64) -> i64 {
    match v {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(default),
        Some(Value::String(s)) => number(s, default),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::TrackRequest;

    fn track(title: &str, artist: &str, album: &str, duration_ms: i64) -> TrackRequest {
        TrackRequest {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            duration_ms,
            source: "lrclib".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: synced candidate, exact title/artist/album, matching duration.
    /// Ground truth (mocked `request_json`, real `adapter_lrclib`): lines are
    /// `[1000/2000/Hello]`, `[3000/177000/World]` — captured in /tmp/parity_lrclib.py.
    #[test]
    fn synced_candidate_parses_to_python_lines() {
        let json = r#"[
            {"id":"a1","trackName":"Test Song","artistName":"Test Artist","albumName":"Test Album","duration":180,"syncedLyrics":"[00:01.00]Hello\n[00:03.00]World","plainLyrics":""},
            {"id":"b2","trackName":"Test Song","artistName":"Other Artist","albumName":"","duration":185,"syncedLyrics":"","plainLyrics":"hello world"}
        ]"#;
        let data: Value = serde_json::from_str(json).unwrap();
        let req = track("Test Song", "Test Artist", "Test Album", 180_000);
        let out = parse_lrclib_response(&data, &req).expect("case A");
        assert_eq!(out.source, "lrclib");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, 1000);
        assert_eq!(out.lines[0].duration_ms, 2000);
        assert_eq!(out.lines[0].text, "Hello");
        assert_eq!(out.lines[0].chars, Vec::<i64>::new());
        assert_eq!(out.lines[1].start_ms, 3000);
        assert_eq!(out.lines[1].duration_ms, 177_000);
        assert_eq!(out.lines[1].text, "World");
    }

    /// Parity case B: matched candidate has only plainLyrics -> parse_plain path.
    /// Ground truth: `[{-1/0/Line one},{-1/0/Line two}]` (captured in /tmp/parity_lrclib2.py).
    #[test]
    fn plain_candidate_parses_to_untimed_lines() {
        let json = r#"[{"id":"p1","trackName":"Plain Song","artistName":"Plain Artist","albumName":"Plain Album","duration":200,"syncedLyrics":"","plainLyrics":"Line one\nLine two"}]"#;
        let data: Value = serde_json::from_str(json).unwrap();
        let req = track("Plain Song", "Plain Artist", "Plain Album", 200_000);
        let out = parse_lrclib_response(&data, &req).expect("case B");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, -1);
        assert_eq!(out.lines[0].duration_ms, 0);
        assert_eq!(out.lines[0].text, "Line one");
        assert_eq!(out.lines[1].start_ms, -1);
        assert_eq!(out.lines[1].duration_ms, 0);
        assert_eq!(out.lines[1].text, "Line two");
    }

    /// Parity case C: no candidates -> "lrclib: no match" (lyric_sources.py:619).
    #[test]
    fn empty_results_yield_no_match() {
        let data: Value = serde_json::from_str("[]").unwrap();
        let req = track("Anything", "Anyone", "", 0);
        let err = parse_lrclib_response(&data, &req).unwrap_err();
        assert_eq!(err, "lrclib: no match");
    }

    /// Parity case D: ranking prefers the synced candidate with stronger identity even when
    /// its duration bucket is worse. Ground truth: selected_candidate_id = "x1".
    #[test]
    fn ranking_prefers_synced_higher_identity() {
        let json = r#"[
            {"id":"x1","trackName":"Rank Song","artistName":"Rank Artist","albumName":"Rank Album","duration":188,"syncedLyrics":"[00:02.00]A\n[00:04.00]B","plainLyrics":"A"},
            {"id":"x2","trackName":"Rank Song","artistName":"Rank Artist","albumName":"","duration":180,"syncedLyrics":"","plainLyrics":"should not be picked"}
        ]"#;
        let data: Value = serde_json::from_str(json).unwrap();
        let req = track("Rank Song", "Rank Artist", "Rank Album", 180_000);
        let out = parse_lrclib_response(&data, &req).expect("case D");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].text, "A");
        assert_eq!(out.lines[0].start_ms, 2000);
        assert_eq!(out.lines[1].text, "B");
        assert_eq!(out.lines[1].start_ms, 4000);
    }

    /// Live LRCLIB smoke test — disabled by default (needs network). Run with
    /// `cargo test -- --ignored lyricfetch::lrclib::tests::live_smoke`.
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "Michael Jackson", "Bad", 257_000);
        let _ = fetch_lrclib(&req).expect("live lrclib fetch");
    }
}
