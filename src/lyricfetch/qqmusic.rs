//! QQ Music source adapter — clean-room port of `lyric_sources.py::adapter_qqmusic`
//! (lines 666-698). Searches `client_search_cp` (`data.song.list`) for song candidates,
//! best-matches one (`songname` / `" ".join(singer.name)` / `albumname`), then fetches
//! `fcg_query_lyric_new.fcg` whose `lyric` / `trans` / `roma` fields are base64-encoded LRC
//! blobs. Python's `TIME_TAG = re.compile(r'\[\d')` guard skips the decode when a field is
//! already a plain `[mm:ss]`-tagged LRC (the endpoint sometimes honours `nobase64=1`).
//! Translation (`trans`) and romanisation (`roma`) fold into the main lines via
//! [`merge_timed`] (168-183), shared with the NetEase adapter.

use base64::Engine;
use serde_json::Value;

use super::lrc::parse_lrc;
use super::netease::{merge_timed, MergeField};
use super::{best_match, finalize, json_str, query_url, request_json, REQUEST_TIMEOUT};
use crate::lyrics::{LyricData, TrackRequest};

const QQ_SEARCH_REFERER: &str = "https://y.qq.com/";
const QQ_LYRIC_REFERER: &str = "https://y.qq.com/portal/player.html";

/// Adapter entry: search + get lyrics for `req`. Mirrors `adapter_qqmusic` (:666-698).
pub(crate) fn fetch_qqmusic(req: &TrackRequest) -> Result<LyricData, String> {
    // search term = non-empty title/artist joined by a space (:668)
    let term = [req.title.clone(), req.artist.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let search_url = query_url(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp",
        &[
            ("format", "json".to_string()),
            ("p", "1".to_string()),
            ("n", "10".to_string()),
            ("w", term),
        ],
    ); // :670
    let search = request_json(
        &search_url,
        Some(&[("Referer", QQ_SEARCH_REFERER)]),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("qqmusic: {e}"))?;
    // candidates = data.song.list (:672-675)
    let songs: Vec<&Value> = search
        .get("data")
        .and_then(|d| d.get("song"))
        .and_then(|s| s.get("list"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    // best_match keys: songname / " ".join(singer.name) / albumname (:677-679)
    let idx = best_match(
        &songs,
        &req.title,
        &req.artist,
        &req.album,
        |x: &Value| json_str(x, "songname"),
        singers_join,
        |x: &Value| json_str(x, "albumname"),
    )
    .ok_or_else(|| "qqmusic: no match".to_string())?; // :681
    let best = songs[idx];
    let songmid = json_str(best, "songmid");
    if songmid.is_empty() {
        return Err("qqmusic: no songmid".to_string());
    }
    let lyric_url = query_url(
        "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg",
        &[
            ("songmid", songmid),
            ("format", "json".to_string()),
            ("nobase64", "1".to_string()),
            ("g_tk", "5381".to_string()),
        ],
    ); // :684
    let lyric = request_json(
        &lyric_url,
        Some(&[("Referer", QQ_LYRIC_REFERER)]),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("qqmusic: {e}"))?;
    parse_qqmusic_response(&lyric, req)
}

/// HTTP-free lyric parse + merge — the testable core. `lyric` is the
/// `fcg_query_lyric_new.fcg` JSON; `req.duration_ms` drives the final `finalize` duration.
fn parse_qqmusic_response(lyric: &Value, req: &TrackRequest) -> Result<LyricData, String> {
    let node = lyric_node(lyric);
    let main_text = decode_field(node, "lyric"); // :688
    let mut lines = parse_lrc(&main_text);
    let translation = parse_lrc(&decode_field(node, "trans")); // :689
    let romanization = parse_lrc(&decode_field(node, "roma")); // :690
    merge_timed(&mut lines, &translation, MergeField::Translation, 500);
    merge_timed(&mut lines, &romanization, MergeField::Romanization, 500);
    let total = req.duration_ms;
    let lines = finalize(lines, total); // success(...) -> finalize(lines, total)
    if lines.is_empty() {
        return Err("qqmusic: no lyrics".to_string()); // success([]) -> empty(...)
    }
    Ok(LyricData {
        source: "qqmusic".to_string(),
        lines,
    })
}

/// The `lyric`/`trans`/`roma` fields sit either at the response root (some `nobase64=1`
/// replies) or under a nested `data` object — pick whichever carries the `lyric` key. Both
/// shapes occur across QQ's endpoint revisions; Python's `data.get("lyric")`-style access
/// only sees the root, so this is a strict superset that stays parity on the common path.
fn lyric_node(data: &Value) -> &Value {
    if data.get("lyric").is_some() {
        data
    } else {
        data.get("data").unwrap_or(data)
    }
}

/// `" ".join(s.get("name","") for s in x.get("singer",[]))` (:678) — QQ song singer names
/// joined by a space (a missing name contributes "" just like Python).
fn singers_join(x: &Value) -> String {
    match x.get("singer").and_then(|s| s.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|s| json_str(s, "name"))
            .collect::<Vec<_>>()
            .join(" "),
        None => String::new(),
    }
}

/// `decoded(key)` (lyric_sources.py:~688): read the field, and if it's already a
/// `[mm:ss]`-tagged LRC use it verbatim (`TIME_TAG.search(value)`), else base64-decode it.
/// Empty / missing fields collapse to `""` so `parse_lrc` yields no lines.
fn decode_field(data: &Value, key: &str) -> String {
    let value = match data.get(key).and_then(|v| v.as_str()) {
        Some(s) => s.trim(),
        None => return String::new(),
    };
    if value.is_empty() {
        return String::new();
    }
    // Python `TIME_TAG = re.compile(r'\[\d')`: a '[' immediately followed by an ASCII digit.
    if has_time_tag(value) {
        return value.to_string();
    }
    match base64::engine::general_purpose::STANDARD.decode(value) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => String::new(),
    }
}

/// `r'\[\d'` over the raw string — byte-window scan avoids building a regex per call.
fn has_time_tag(s: &str) -> bool {
    for w in s.as_bytes().windows(2) {
        if w[0] == b'[' && w[1].is_ascii_digit() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::TrackRequest;
    use base64::Engine;

    fn track(title: &str, artist: &str, album: &str, duration_ms: i64) -> TrackRequest {
        TrackRequest {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            duration_ms,
            source: "qqmusic".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: base64-encoded `lyric` + `trans` under the `data` wrapper; `roma`
    /// empty. Ground truth (mocked request_json, real adapter_qqmusic) recorded in
    /// /tmp/parity_qqmusic.py: `[1000/2000/Hello->你好]`, `[3000/197000/World->世界]`.
    #[test]
    fn base64_lrc_merges_translation() {
        let lrc = "[00:01.00]Hello\n[00:03.00]World";
        let trans = "[00:01.00]你好\n[00:03.00]世界";
        let enc = base64::engine::general_purpose::STANDARD.encode(lrc.as_bytes());
        let tenc = base64::engine::general_purpose::STANDARD.encode(trans.as_bytes());
        let json = format!(
            r#"{{"code":0,"data":{{"lyric":"{enc}","trans":"{tenc}","roma":""}}}}"#
        );
        let lyric: Value = serde_json::from_str(&json).unwrap();
        let req = track("Test Song", "Test Artist", "Test Album", 200_000);
        let out = parse_qqmusic_response(&lyric, &req).expect("case A");
        assert_eq!(out.source, "qqmusic");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, 1000);
        assert_eq!(out.lines[0].duration_ms, 2000);
        assert_eq!(out.lines[0].text, "Hello");
        assert_eq!(out.lines[0].translation, "你好");
        assert_eq!(out.lines[0].romanization, "");
        assert_eq!(out.lines[1].start_ms, 3000);
        assert_eq!(out.lines[1].duration_ms, 197_000);
        assert_eq!(out.lines[1].text, "World");
        assert_eq!(out.lines[1].translation, "世界");
    }

    /// Parity case B: a `lyric` value already carrying a `[00:` time tag is used verbatim
    /// (no base64 decode) — Python's `TIME_TAG` guard (:688).
    #[test]
    fn tagged_lyric_skips_decode() {
        let json = r#"{"lyric":"[00:02.00]Plain line"}"#;
        let lyric: Value = serde_json::from_str(json).unwrap();
        let req = track("T", "A", "", 200_000);
        let out = parse_qqmusic_response(&lyric, &req).unwrap();
        assert_eq!(out.lines.len(), 1);
        assert_eq!(out.lines[0].text, "Plain line");
        assert_eq!(out.lines[0].start_ms, 2000);
    }

    /// No lyric content at all -> "qqmusic: no lyrics" (mirrors success([])->empty(...)).
    #[test]
    fn empty_lyrics_yield_err() {
        let lyric: Value = serde_json::from_str(r#"{"data":{"lyric":""}}"#).unwrap();
        let req = track("T", "A", "", 200_000);
        let err = parse_qqmusic_response(&lyric, &req).unwrap_err();
        assert_eq!(err, "qqmusic: no lyrics");
    }

    /// Live QQ smoke test — disabled by default (needs network).
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "迈克尔杰克逊", "", 257_000);
        let _ = fetch_qqmusic(&req).expect("live qqmusic fetch");
    }
}
