//! SPlayer source adapter — clean-room port of `lyric_sources.py::adapter_splayer`
//! (lines 700-748) and its word-level parser `splayer_transmitted_lines` (73-130). SPlayer is
//! a local companion app exposing the currently-playing track at `/api/control/song-info`;
//! the adapter polls it up to 3 times (400ms apart) and, once the on-screen track matches the
//! requested one, folds the app's pushed `yrcData`/`lrcData` (word-timed JSON, not LRC text)
//! into [`LyricLine`]s via [`parse_transmitted`]. Credentials in Python came from
//! `credentials.get("splayer_api_url")`; with no credentials surface in Rust we read the URL
//! from the `SPLAYER_API_URL` env var, defaulting to the documented loopback endpoint.
//!
//! Parity scope: the `words` / `is_background` / `is_duet` side channels and the returned
//! cover have no home in [`crate::lyrics::LyricData`] (it carries only `source` + `lines`), so
//! they are intentionally not ported; only `text` / `translation` / `romanization` / `chars`
//! survive. The `[mm:ss]` LRC text path (`parse_lrc`) is unused — SPlayer is JSON-only.

use std::thread::sleep;
use std::time::Duration;

use serde_json::Value;

use super::{clean_text, finalize, json_num, json_str, normalize, request_json};
use crate::lyrics::{LyricData, LyricLine, TrackRequest};

/// Documented loopback SPlayer API (lyric_sources.py:704 default when no credential present).
const SPLAYER_DEFAULT_URL: &str = "http://127.0.0.1:25884";

/// Adapter entry: poll the local SPlayer app for the on-screen track's lyrics.
/// Mirrors `adapter_splayer` (:700-748).
pub(crate) fn fetch_splayer(req: &TrackRequest) -> Result<LyricData, String> {
    let base_url = std::env::var("SPLAYER_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| clean_text(&s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SPLAYER_DEFAULT_URL.to_string());
    if !valid_http_url(&base_url) {
        return Err("splayer: invalid API URL".to_string()); // :707
    }
    let endpoint = format!(
        "{}/api/control/song-info",
        base_url.trim_end_matches('/')
    ); // :712

    let title = clean_text(&req.title);
    let artist = clean_text(&req.artist);
    let mut last_state = "unavailable"; // :713
    for attempt in 0..3u32 {
        match request_json(&endpoint, None, Duration::from_secs(1)) {
            Ok(response) => {
                let empty = Value::Object(serde_json::Map::new());
                // current = response.get("data", {}) if dict else {} (:715)
                let current = response.get("data").unwrap_or(&empty);
                let current_title = first_present_str(current, &["name", "playName"]); // :716
                let current_artist = artist_field(current); // :717-721
                let wanted_title = normalize(&title);
                let normalized_title = normalize(&current_title);
                let title_matches = normalized_title == wanted_title
                    || (!normalized_title.is_empty()
                        && (wanted_title.contains(&normalized_title)
                            || normalized_title.contains(&wanted_title))); // :724-726
                let artist_matches = artist.is_empty()
                    || current_artist.is_empty()
                    || {
                        let a = normalize(&artist);
                        let c = normalize(&current_artist);
                        a.contains(&c) || c.contains(&a)
                    }; // :727-729
                if title_matches && artist_matches {
                    let transmitted = parse_transmitted(current); // :731
                    if !transmitted.is_empty() {
                        // success(source, lines, ..., expected_duration, cover) — re-finalize
                        // with the track duration (Python's success double-finalize).
                        let lines = finalize(transmitted, req.duration_ms);
                        return Ok(LyricData {
                            source: "splayer".to_string(),
                            lines,
                        }); // :734-738
                    }
                    last_state =
                        if current.get("lyricLoading").and_then(|v| v.as_bool()) == Some(true) {
                            "loading"
                        } else {
                            "empty"
                        }; // :739-740
                } else {
                    last_state = "track not ready"; // :742
                }
            }
            Err(_) => last_state = "API unavailable", // :743
        }
        if attempt < 2 {
            sleep(Duration::from_millis(400)); // :745
        }
    }
    Err(format!("splayer: {last_state}")) // :747
}

/// `splayer_transmitted_lines` (lyric_sources.py:73-96): pick `yrcData` then `lrcData`, parse
/// whichever carries lines, finalize against the app-reported duration. Empty / non-dict data
/// yields no lines so the caller advances `last_state`.
fn parse_transmitted(data: &Value) -> Vec<LyricLine> {
    if !data.is_object() {
        return Vec::new(); // :75 `isinstance(data, dict)`
    }
    let total = json_num(data.get("duration"), 0); // `number(data.get("duration"), 0)`
    for key in ["yrcData", "lrcData"] {
        if let Some(source_lines) = data.get(key).and_then(|v| v.as_array()) {
            let parsed = parse_lines(source_lines, total);
            if !parsed.is_empty() {
                return parsed; // :94-95
            }
        }
    }
    Vec::new() // :96
}

/// Inner `parse_lines` (lyric_sources.py:77-93): turn one `yrcData`/`lrcData` array of
/// word-timed dicts into [`LyricLine`]s, folding per-word `romanWord` into the line's
/// romanisation and per-word timings into `chars`. The single-word/long-line heuristic
/// (:124-130) clears `chars` when one word spans the whole line with a tight gap to the next.
fn parse_lines(source_lines: &[Value], total: i64) -> Vec<LyricLine> {
    let n = source_lines.len();
    let mut result: Vec<LyricLine> = Vec::new();
    for (i, source_line) in source_lines.iter().enumerate() {
        let sl = match source_line.as_object() {
            Some(o) => o,
            None => continue, // :79
        };
        let start = json_num(sl.get("startTime"), -1); // :80
        let end = json_num(sl.get("endTime"), start); // :81
        let words = sl.get("words").and_then(|v| v.as_array());

        let mut text_parts: Vec<String> = Vec::new();
        let mut roman_parts: Vec<String> = Vec::new();
        let mut chars: Vec<i64> = Vec::new();
        // word_timings[0] start/end (captured for the duration_inferred heuristic).
        let mut first_word_span: Option<(i64, i64)> = None;
        if let Some(words) = words {
            for word in words {
                let w = match word.as_object() {
                    Some(o) => o,
                    None => continue, // :85
                };
                let raw = match w.get("word") {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                let text = clean_text(&raw); // html.unescape + BOM strip (:86)
                if text.is_empty() {
                    continue; // :87
                }
                let word_start = json_num(w.get("startTime"), start); // :88
                let word_end = json_num(w.get("endTime"), word_start); // :89
                text_parts.push(text.clone());
                if first_word_span.is_none() {
                    first_word_span = Some((word_start, word_end));
                }
                let roman_word = {
                    let r = json_str(word, "romanWord");
                    if !r.is_empty() {
                        r
                    } else {
                        json_str(word, "romanization")
                    }
                }; // :91
                if !roman_word.is_empty() {
                    roman_parts.push(roman_word.clone());
                }
                // chars += word_start + i * dur // len(text) for i in 0..len(text) (:93)
                let count = text.chars().count() as i64;
                let tlen = count.max(1);
                let dur = (word_end - word_start).max(0);
                for index in 0..count {
                    chars.push(word_start + index * dur / tlen);
                }
            }
        }

        let text = if !text_parts.is_empty() {
            text_parts.join("")
        } else {
            let t = json_str(source_line, "text");
            if !t.is_empty() {
                t
            } else {
                json_str(source_line, "lyric")
            }
        }; // :95
        let translation = {
            let t = json_str(source_line, "translatedLyric");
            if !t.is_empty() {
                t
            } else {
                json_str(source_line, "translation")
            }
        }; // :96
        let romanization = {
            let r = json_str(source_line, "romanLyric");
            let r2 = if !r.is_empty() {
                r
            } else {
                json_str(source_line, "romanization")
            };
            if !r2.is_empty() {
                r2
            } else {
                roman_parts.join(" ")
            }
        }; // :97

        let mut line_chars = chars;
        // duration_inferred heuristic (:124-130): single word, line >=7s, tight to next line
        // and that word spans [start,end] within 50ms → drop the (spurious) chars.
        if text_parts.len() == 1 && (end - start) >= 7000 {
            let next_start = match source_lines
                .get(i + 1)
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("startTime"))
            {
                Some(v) => json_num(Some(v), -1),
                None => -1,
            };
            if (next_start - end).abs() <= 50 {
                if let Some((ws, we)) = first_word_span {
                    if (ws - start).abs() <= 50 && (we - end).abs() <= 50 {
                        line_chars.clear(); // :129
                    }
                }
            }
        }

        let item = LyricLine {
            start_ms: start,
            duration_ms: (end - start).max(0),
            text,
            translation,
            romanization,
            chars: line_chars,
            words: Vec::new(),
            song_part: String::new(),
            block_index: 0,
            chorus_flag: false,
        };
        if !item.text.is_empty()
            || !item.translation.is_empty()
            || !item.romanization.is_empty()
        {
            result.push(item); // :131
        }
    }
    finalize(result, total) // :133
}

/// `current.get("name", current.get("playName", ""))` (:716) — honor key presence so an
/// explicit empty-string value wins over the fallback (matches Python's eager-default).
fn first_present_str(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(val) = v.get(k) {
            return match val {
                Value::String(s) => clean_text(s),
                other => clean_text(&other.to_string()),
            };
        }
    }
    String::new()
}

/// `current.get("artistName", current.get("artist", current.get("artists", "")))` with the
/// list-join special case (:717-721): a list of names is joined by a space; a bare dict item
/// contributes its `name` (or its string form when `name` is absent, parity with Python).
fn artist_field(current: &Value) -> String {
    let raw = current
        .get("artistName")
        .or_else(|| current.get("artist"))
        .or_else(|| current.get("artists"));
    match raw {
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|item| match item {
                Value::Object(o) => {
                    let n = match o.get("name") {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => item.to_string(),
                    };
                    clean_text(&n)
                }
                Value::String(s) => clean_text(s),
                other => clean_text(&other.to_string()),
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some(Value::String(s)) => clean_text(s),
        Some(other) => clean_text(&other.to_string()),
        None => String::new(),
    }
}

/// `urllib.parse.urlsplit(base_url)` scheme/netloc check (:705-706) — inline since `url` is
/// not a dependency. Schemes are case-insensitive (`http`/`https`); netloc runs up to the
/// first path/query/fragment separator.
fn valid_http_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let after_scheme = if let Some(rest) = lower.strip_prefix("http://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else {
        return false;
    };
    let netloc = after_scheme
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()
        .unwrap_or("");
    !netloc.is_empty()
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
            source: "splayer".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: `yrcData` with one word-timed line folds into a single LyricLine.
    /// Ground truth (real splayer_transmitted_lines over this payload): one line
    /// `1000/2000/HelloWorld` with romanisation "wa" and 10 evenly-spaced char timings.
    #[test]
    fn yrcdata_word_lines_parse() {
        let json = r#"{"duration":200000,"yrcData":[
            {"startTime":1000,"endTime":3000,"words":[
                {"word":"Hello","startTime":1000,"endTime":2000,"romanWord":""},
                {"word":"World","startTime":2000,"endTime":3000,"romanWord":"wa"}
            ]}
        ]}"#;
        let data: Value = serde_json::from_str(json).unwrap();
        let out = parse_transmitted(&data);
        assert_eq!(out.len(), 1);
        let line = &out[0];
        assert_eq!(line.start_ms, 1000);
        assert_eq!(line.duration_ms, 2000);
        assert_eq!(line.text, "HelloWorld");
        assert_eq!(line.translation, "");
        assert_eq!(line.romanization, "wa");
        assert_eq!(
            line.chars,
            vec![1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800]
        );
    }

    /// Parity case B: `lrcData` fallback when `yrcData` is empty/missing.
    #[test]
    fn lrcdata_fallback() {
        let json = r#"{"duration":100000,"yrcData":[],"lrcData":[
            {"startTime":2000,"endTime":3000,"text":"Plain line"}
        ]}"#;
        let data: Value = serde_json::from_str(json).unwrap();
        let out = parse_transmitted(&data);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_ms, 2000);
        assert_eq!(out[0].duration_ms, 1000);
        assert_eq!(out[0].text, "Plain line");
    }

    /// Parity case C: non-dict data -> no lines (splayer_transmitted_lines guard, :75).
    #[test]
    fn non_dict_yields_nothing() {
        let data: Value = serde_json::from_str("[]").unwrap();
        assert!(parse_transmitted(&data).is_empty());
    }

    /// URL validation parity with `urllib.parse.urlsplit` (:705-706).
    #[test]
    fn url_validator() {
        assert!(valid_http_url("http://127.0.0.1:25884"));
        assert!(valid_http_url("https://example.com/path"));
        assert!(valid_http_url("HTTPS://Example.com"));
        assert!(!valid_http_url("ftp://example.com"));
        assert!(!valid_http_url("file:///x"));
        assert!(!valid_http_url("http:///no-host"));
        assert!(!valid_http_url("not-a-url"));
    }

    /// Live SPlayer smoke test — disabled by default (needs the local app running).
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "Michael Jackson", "", 257_000);
        let _ = fetch_splayer(&req).expect("live splayer fetch");
    }
}
