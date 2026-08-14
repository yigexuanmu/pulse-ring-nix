//! NetEase source adapter — clean-room port of `lyric_sources.py::adapter_netease` (lines
//! 631-664). Searches `/api/search/get`, best-matches a track, then pulls
//! `/api/song/lyric` (yrc -> klyric -> lrc, first non-empty parse wins) and folds
//! translation (`tlyric`) and romanisation (`romalrc`) in via [`merge_timed`] (168-183).
//! Cover / `picUrl` rewriting is not ported — `LyricData` carries only `source` + `lines`.

use crate::lyrics::{LyricData, LyricLine, TrackRequest};
use serde_json::Value;
use super::{best_match, finalize, json_num, json_str, query_url, request_json, REQUEST_TIMEOUT};
use super::lrc::parse_lrc;

const NETEASE_REFERER: &str = "https://music.163.com/";

/// Adapter entry: search + get lyrics for `req`. Mirrors `adapter_netease` (:631-664).
pub(crate) fn fetch_netease(req: &TrackRequest) -> Result<LyricData, String> {
    // search term = non-empty title/artist joined by a space (:633)
    let term = [req.title.clone(), req.artist.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let search_url = query_url(
        "https://music.163.com/api/search/get",
        &[
            ("type", "1".to_string()),
            ("s", term),
            ("limit", "10".to_string()),
        ],
    );
    let search = request_json(
        &search_url,
        Some(&[("Referer", NETEASE_REFERER)]),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("netease: {e}"))?;
    let songs: Vec<&Value> = search
        .get("result")
        .and_then(|r| r.get("songs"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    // best_match keys: name / " ".join(artists[].name) / album.name (:637-639)
    let idx = best_match(
        &songs,
        &req.title,
        &req.artist,
        &req.album,
        |x: &Value| json_str(x, "name"),
        artists_join,
        |x: &Value| x.get("album").map(|a| json_str(a, "name")).unwrap_or_default(),
    )
    .ok_or_else(|| "netease: no match".to_string())?; // :641
    let best = songs[idx];
    let song_id = json_num(best.get("id"), 0);
    let lyric_url = query_url(
        "https://music.163.com/api/song/lyric",
        &[
            ("id", song_id.to_string()),
            ("lv", "1".to_string()),
            ("kv", "1".to_string()),
            ("tv", "1".to_string()),
            ("rv", "1".to_string()),
            ("yv", "1".to_string()),
        ],
    );
    let lyric = request_json(
        &lyric_url,
        Some(&[("Referer", NETEASE_REFERER)]),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("netease: {e}"))?;
    parse_netease_response(&lyric, req)
}

/// HTTP-free lyric parse + merge — the testable core. `lyric` is the `/api/song/lyric`
/// response; `req.duration_ms` drives the final `finalize` duration.
fn parse_netease_response(lyric: &Value, req: &TrackRequest) -> Result<LyricData, String> {
    // yrc -> klyric -> lrc, first non-empty parse wins (:645-652). `lines` is reset every
    // iteration (parse_lrc(main) if main else []) and breaks once it is non-empty.
    let mut lines: Vec<LyricLine> = Vec::new();
    for name in ["yrc", "klyric", "lrc"] {
        let main = lyric_field(lyric, name);
        lines = if main.is_empty() { Vec::new() } else { parse_lrc(&main) };
        if !lines.is_empty() {
            break;
        }
    }
    let translation = parse_lrc(&lyric_field(lyric, "tlyric")); // :653-654
    let romanization = parse_lrc(&lyric_field(lyric, "romalrc")); // :655-656
    merge_timed(&mut lines, &translation, MergeField::Translation, 500);
    merge_timed(&mut lines, &romanization, MergeField::Romanization, 500);
    let total = req.duration_ms; // == Python's duration_ms(track.duration)
    let lines = finalize(lines, total); // success(...) -> finalize(lines, total)
    if lines.is_empty() {
        return Err("netease: no lyrics".to_string()); // success([]) -> empty(...)
    }
    Ok(LyricData { source: "netease".to_string(), lines })
}

/// `main = data.get(name, {})` then `.get("lyric","") if dict else main` (:646-647): return
/// the lyric text under `name` whether the node is a dict or a bare string.
fn lyric_field(data: &Value, name: &str) -> String {
    match data.get(name) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("lyric")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// `" ".join(a.get("name","") for a in x.get("artists",[]))` (:638) — NetEase song artist
/// names joined by a space (a missing name contributes "" just like Python).
fn artists_join(x: &Value) -> String {
    match x.get("artists").and_then(|a| a.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|a| json_str(a, "name"))
            .collect::<Vec<_>>()
            .join(" "),
        None => String::new(),
    }
}

/// Which [`LyricLine`] side-channel [`merge_timed`] writes into.
enum MergeField {
    Translation,
    Romanization,
}

/// `merge_timed` (lyric_sources.py:168-183): fold `secondary` into `primary` line-by-line.
/// For each target line, copy the closest-by-time *timed* secondary line (within
/// `tolerance` ms) — or the index-aligned *untimed* one — but only if the target's field is
/// still empty. Mutates `primary` in place; ties on the closest match keep the first
/// secondary line (Python `min`).
fn merge_timed(
    primary: &mut [LyricLine],
    secondary: &[LyricLine],
    field: MergeField,
    tolerance: i64,
) {
    if primary.is_empty() || secondary.is_empty() {
        return;
    }
    let untimed: Vec<&LyricLine> = secondary.iter().filter(|l| l.start_ms < 0).collect();
    let timed: Vec<&LyricLine> = secondary.iter().filter(|l| l.start_ms >= 0).collect();
    for (index, target) in primary.iter_mut().enumerate() {
        let mut value = String::new();
        if target.start_ms >= 0 && !timed.is_empty() {
            let mut best_idx = 0usize;
            let mut best_diff = (target.start_ms - timed[0].start_ms).abs();
            for (i, cand) in timed.iter().enumerate().skip(1) {
                let d = (target.start_ms - cand.start_ms).abs();
                if d < best_diff {
                    best_diff = d;
                    best_idx = i;
                }
            }
            if (target.start_ms - timed[best_idx].start_ms).abs() <= tolerance {
                value = timed[best_idx].text.clone();
            }
        } else if index < untimed.len() {
            value = untimed[index].text.clone();
        }
        if !value.is_empty() {
            match field {
                MergeField::Translation => {
                    if target.translation.is_empty() {
                        target.translation = value;
                    }
                }
                MergeField::Romanization => {
                    if target.romanization.is_empty() {
                        target.romanization = value;
                    }
                }
            }
        }
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
            source: "netease".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: lrc lyrics + tlyric translation merged by closest time.
    /// Ground truth (mocked request_json, real adapter_netease) in /tmp/parity_netease.py:
    /// `[1000/2000/Hello->Hola]`, `[3000/197000/World->Mundo]`.
    #[test]
    fn lyrics_merge_translation() {
        let json = r#"{"lrc":{"lyric":"[00:01.00]Hello\n[00:03.00]World"},"tlyric":{"lyric":"[00:01.00]Hola\n[00:03.00]Mundo"},"romalrc":{"lyric":""}}"#;
        let lyric: Value = serde_json::from_str(json).unwrap();
        let req = track("Test Song", "Test Artist", "Test Album", 200_000);
        let out = parse_netease_response(&lyric, &req).expect("case A");
        assert_eq!(out.source, "netease");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, 1000);
        assert_eq!(out.lines[0].duration_ms, 2000);
        assert_eq!(out.lines[0].text, "Hello");
        assert_eq!(out.lines[0].translation, "Hola");
        assert_eq!(out.lines[0].romanization, "");
        assert_eq!(out.lines[1].start_ms, 3000);
        assert_eq!(out.lines[1].duration_ms, 197_000);
        assert_eq!(out.lines[1].text, "World");
        assert_eq!(out.lines[1].translation, "Mundo");
    }

    /// Parity case C: klyric wins over lrc when both present (yrc/klyric/lrc precedence).
    /// Ground truth: single line `[5000/195000/Klyric line]`.
    #[test]
    fn klyric_preferred_over_lrc() {
        let json = r#"{"klyric":{"lyric":"[00:05.00]Klyric line"},"lrc":{"lyric":"[00:10.00]Lrc fallback"}}"#;
        let lyric: Value = serde_json::from_str(json).unwrap();
        let req = track("Test Song", "Test Artist", "Test Album", 200_000);
        let out = parse_netease_response(&lyric, &req).expect("case C");
        assert_eq!(out.lines.len(), 1);
        assert_eq!(out.lines[0].start_ms, 5000);
        assert_eq!(out.lines[0].duration_ms, 195_000);
        assert_eq!(out.lines[0].text, "Klyric line");
        assert_eq!(out.lines[0].translation, "");
    }

    /// No lyric fields present -> "netease: no lyrics" (mirrors success([])->empty(...)).
    #[test]
    fn no_lyrics_yields_err() {
        let lyric: Value = serde_json::from_str("{}").unwrap();
        let req = track("Test Song", "Test Artist", "", 200_000);
        let err = parse_netease_response(&lyric, &req).unwrap_err();
        assert_eq!(err, "netease: no lyrics");
    }

    /// Parity case B: empty search songs -> best_match None -> "netease: no match" (:641).
    /// Drives the no-match branch from `fetch_netease` without touching the network.
    #[test]
    fn empty_songs_yield_no_match() {
        let songs: Vec<&Value> = Vec::new();
        assert!(best_match(
            &songs,
            "Test Song",
            "Test Artist",
            "",
            |x| json_str(x, "name"),
            |_| String::new(),
            |_| String::new(),
        )
        .is_none());
    }

    /// Live NetEase smoke test — disabled by default (needs network).
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "Michael Jackson", "", 257_000);
        let _ = fetch_netease(&req).expect("live netease fetch");
    }
}
