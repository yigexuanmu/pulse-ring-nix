//! JSON shape-walking helpers — clean-room port of the JSON-value accessors in
//! `lyric_sources.py` (lines 346-470): `first_value` / `first_value_str` /
//! `first_value_num` / `parse_json_lines` / `parse_payload`.
//!
//! These live apart from the per-source adapters so every source can share the same
//! tolerant "walk a handful of key paths, take the first non-null" lookup plus a single
//! recursive lyric-tree parser that knows the common line shapes (QRC / Musixmatch /
//! Spotify / Apple). The string branch routes TTML/LRC bodies to the dedicated parsers in
//! the `ttml` / `lrc` sibling modules.

use crate::lyrics::LyricLine;

/// `first_value` (lyric_sources.py:460-470): walk each `paths` entry (a sequence of keys)
/// over the JSON value, returning the first path that resolves to a non-null value.
pub(crate) fn first_value<'a>(
    v: &'a serde_json::Value,
    paths: &[&[&str]],
) -> Option<&'a serde_json::Value> {
    for path in paths {
        let mut cur = v;
        for k in *path {
            match cur.get(k) {
                Some(n) if !n.is_null() => cur = n,
                _ => {
                    cur = &serde_json::Value::Null;
                    break;
                }
            }
        }
        if !cur.is_null() {
            return Some(cur);
        }
    }
    None
}

/// `first_value_str`: `first_value` coerced to a cleaned string (mirrors `clean_text(j.get(k))`).
pub(crate) fn first_value_str(v: &serde_json::Value, paths: &[&[&str]]) -> String {
    match first_value(v, paths) {
        Some(serde_json::Value::String(s)) => super::clean_text(s),
        Some(other) => super::clean_text(&other.to_string()),
        None => String::new(),
    }
}

/// `first_value_num`: `first_value` coerced to an integer (JSON number or numeric string).
pub(crate) fn first_value_num(v: &serde_json::Value, paths: &[&[&str]]) -> i64 {
    match first_value(v, paths) {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0) as i64,
        Some(serde_json::Value::String(s)) => super::number(s, 0),
        _ => 0,
    }
}

/// `parse_json_lines` (lyric_sources.py:346-398): recursively dig a lyric line list out of
/// an arbitrary JSON value. Strings route to the TTML/LRC/plain parsers by smell; objects
/// probe the common container keys (`lines`/`lyricLines`/`syncedLyrics`/`ttml`/`lyric`/...);
/// arrays are the actual line lists once we reach them.
pub(crate) fn parse_json_lines(value: &serde_json::Value) -> Vec<LyricLine> {
    match value {
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with('<')
                && (trimmed.contains("<tt") || trimmed.contains("<p"))
            {
                super::ttml::parse_ttml(trimmed)
            } else if trimmed.contains('[') {
                super::lrc::parse_lrc(trimmed)
            } else {
                super::lrc::parse_plain(trimmed)
            }
        }
        serde_json::Value::Object(o) => {
            for key in &["lines", "lyricLines", "lyricsLines", "sentences"] {
                if let Some(v) = o.get(*key) {
                    if !v.is_null() {
                        let parsed = parse_json_lines(v);
                        if !parsed.is_empty() {
                            return parsed;
                        }
                    }
                }
            }
            for key in &[
                "syncedLyrics",
                "synced_lyrics",
                "subtitle_body",
                "ttml",
                "lyric",
                "lyrics",
                "lrc",
                "content",
            ] {
                if let Some(v) = o.get(*key) {
                    if !v.is_null() {
                        return parse_json_lines(v);
                    }
                }
            }
            Vec::new()
        }
        serde_json::Value::Array(arr) => {
            let mut result: Vec<LyricLine> = Vec::new();
            for item in arr {
                match item {
                    serde_json::Value::String(s) => result.push(LyricLine {
                        start_ms: -1,
                        duration_ms: 0,
                        text: super::clean_text(s),
                        translation: String::new(),
                        romanization: String::new(),
                        chars: Vec::new(),
                        words: Vec::new(),
                        song_part: String::new(),
                        block_index: 0,
                        chorus_flag: false,
                    }),
                    serde_json::Value::Object(o) => {
                        let start = first_value_num(
                            item,
                            &[
                                &["time"],
                                &["start"],
                                &["startTime"],
                                &["startTimeMs"],
                                &["start_time"],
                                &["begin"],
                                &["timestamp"],
                            ],
                        );
                        let end =
                            first_value_num(item, &[&["end"], &["endTime"], &["end_time"]]);
                        let duration = first_value_num(
                            item,
                            &[&["duration"], &["durationMs"], &["duration_ms"]],
                        );
                        let text = first_value_str(
                            item,
                            &[
                                &["text"],
                                &["words"],
                                &["lyric"],
                                &["content"],
                                &["line"],
                            ],
                        );
                        let translation = first_value_str(
                            item,
                            &[
                                &["translation"],
                                &["translated"],
                                &["translatedLyric"],
                            ],
                        );
                        let romanization = first_value_str(
                            item,
                            &[
                                &["romanization"],
                                &["romanized"],
                                &["romaji"],
                                &["transliteration"],
                            ],
                        );
                        let mut chars: Vec<i64> = Vec::new();
                        for chkey in &["chars", "charTimes", "syllables", "wordsTiming"] {
                            if let Some(arr) = o.get(*chkey).filter(|v| v.is_array()) {
                                for c in arr.as_array().unwrap() {
                                    let t = match c {
                                        serde_json::Value::Object(_) => first_value_num(
                                            c,
                                            &[&["time"], &["start"], &["startTimeMs"]],
                                        ),
                                        serde_json::Value::Number(n) => {
                                            n.as_f64().unwrap_or(0.0) as i64
                                        }
                                        _ => 0,
                                    };
                                    chars.push(t);
                                }
                                break;
                            }
                        }
                        let duration = if duration == 0 && end > start {
                            (end - start).max(0)
                        } else {
                            duration
                        };
                        result.push(LyricLine {
                            start_ms: start,
                            duration_ms: duration,
                            text,
                            translation,
                            romanization,
                            chars,
                            words: Vec::new(),
                            song_part: String::new(),
                            block_index: 0,
                            chorus_flag: false,
                        });
                    }
                    _ => {}
                }
            }
            super::finalize(result, 0)
        }
        _ => Vec::new(),
    }
}

/// `parse_payload` (lyric_sources.py:399-413): sniff a raw textual payload and route it to
/// the TTML / LRC / plain parser — used by adapters that fetch a string body (Apple's ttml
/// field, Musixmatch's `subtitle_body`, plain LRC over the wire).
pub(crate) fn parse_payload(text: &str) -> Vec<LyricLine> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.starts_with('<') {
        return super::ttml::parse_ttml(trimmed);
    }
    if trimmed.contains('[') {
        let lines = super::lrc::parse_lrc(trimmed);
        if !lines.is_empty() {
            return lines;
        }
    }
    super::lrc::parse_plain(trimmed)
}
