//! Kugou source adapter — clean-room port of `lyric_sources.py::adapter_kugou` (lines
//! 749-781). Two-step fetch: first `/api/v3/search/song` for song candidates (`data.info`),
//! best-match one (`songname||filename` / `singername` / `album_name`), then
//! `lyrics.kugou.com/search` for lyric candidates (`candidates[]`) and finally
//! `lyrics.kugou.com/download` whose `content` is a base64-encoded LRC blob. Python keeps the
//! raw string on a base64 error (`try/except (ValueError, TypeError): pass`) rather than
//! failing — preserved here via [`decode_content`]'s fallback. No translation / romanisation
//! fold (Kugou returns plain LRC), so the result is a `parse_lrc` + `finalize` pass.

use base64::Engine;
use serde_json::Value;

use super::lrc::parse_lrc;
use super::{best_match, finalize, json_num, json_str, query_url, request_json, REQUEST_TIMEOUT};
use crate::lyrics::{LyricData, TrackRequest};

/// Adapter entry: search + resolve + download lyrics for `req`. Mirrors `adapter_kugou`
/// (:749-781).
pub(crate) fn fetch_kugou(req: &TrackRequest) -> Result<LyricData, String> {
    // keyword = non-empty title/artist joined by a space (:751)
    let keyword = [req.title.clone(), req.artist.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let search_url = query_url(
        "https://mobilecdn.kugou.com/api/v3/search/song",
        &[
            ("format", "json".to_string()),
            ("keyword", keyword.clone()),
            ("page", "1".to_string()),
            ("pagesize", "10".to_string()),
            ("showtype", "1".to_string()),
        ],
    ); // :753-756
    let search =
        request_json(&search_url, None, REQUEST_TIMEOUT).map_err(|e| format!("kugou: {e}"))?;
    let songs: Vec<&Value> = search
        .get("data")
        .and_then(|d| d.get("info"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default(); // :757 `data.info`
    // best_match keys: songname||filename / singername / album_name (:758-760)
    let idx = best_match(
        &songs,
        &req.title,
        &req.artist,
        &req.album,
        |x: &Value| {
            let s = json_str(x, "songname");
            if !s.is_empty() {
                s
            } else {
                json_str(x, "filename")
            }
        },
        |x: &Value| json_str(x, "singername"),
        |x: &Value| json_str(x, "album_name"),
    )
    .ok_or_else(|| "kugou: no match".to_string())?; // :762
    let best = songs[idx];

    // candidates request needs the search result's `duration` (falling back to the track
    // duration in ms) and `hash`. (:766-769)
    let best_duration = json_num(best.get("duration"), req.duration_ms);
    let hash = json_str(best, "hash");
    let cand_url = query_url(
        "https://lyrics.kugou.com/search",
        &[
            ("ver", "1".to_string()),
            ("man", "yes".to_string()),
            ("client", "pc".to_string()),
            ("keyword", keyword.clone()),
            ("duration", best_duration.to_string()),
            ("hash", hash),
        ],
    ); // :766
    let cand_resp =
        request_json(&cand_url, None, REQUEST_TIMEOUT).map_err(|e| format!("kugou: {e}"))?;
    let candidates: Vec<&Value> = cand_resp
        .get("candidates")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default(); // :770
    if candidates.is_empty() {
        return Err("kugou: lyrics unavailable".to_string()); // :771-772
    }
    let candidate = candidates[0]; // :773
    let id = value_str(candidate, "id"); // numeric -> string, no clean (must stay intact)
    let accesskey = value_str(candidate, "accesskey");
    let dl_url = query_url(
        "https://lyrics.kugou.com/download",
        &[
            ("ver", "1".to_string()),
            ("client", "pc".to_string()),
            ("id", id),
            ("accesskey", accesskey),
            ("fmt", "lrc".to_string()),
            ("charset", "utf8".to_string()),
        ],
    ); // :774-776
    let dl = request_json(&dl_url, None, REQUEST_TIMEOUT).map_err(|e| format!("kugou: {e}"))?;
    parse_kugou_response(&dl, req.duration_ms)
}

/// HTTP-free download parse — the testable core. `dl` is the `lyrics.kugou.com/download`
/// JSON (`content` field); `duration_ms` drives the final `finalize` (success double-finalize).
fn parse_kugou_response(dl: &Value, duration_ms: i64) -> Result<LyricData, String> {
    let content = dl.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let decoded = decode_content(content); // :778-781
    let lines = finalize(parse_lrc(&decoded), duration_ms);
    if lines.is_empty() {
        return Err("kugou: no lyrics".to_string()); // success([])->empty(...)
    }
    Ok(LyricData {
        source: "kugou".to_string(),
        lines,
    })
}

/// `base64.b64decode(content)` with a `try/except (ValueError, TypeError): pass` guard
/// (lyric_sources.py:778-781): on a malformed base64 blob keep the raw string (it may
/// already be a plain LRC). Empty input collapses to `""`.
fn decode_content(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    match base64::engine::general_purpose::STANDARD.decode(content) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => content.to_string(),
    }
}

/// Read a JSON field as a verbatim string — numbers stringify, without `clean_text` so an
/// integer `id` and a base64-ish `accesskey` survive byte-for-byte for the download request.
fn value_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
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
            source: "kugou".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: base64-encoded LRC `content` decodes to two timed lines.
    /// Ground truth (adapter_kugou over this download payload) in /tmp/parity_kugou.py:
    /// `[1000/2000/Hello]`, `[3000/197000/World]`.
    #[test]
    fn base64_content_parses() {
        let lrc = "[00:01.00]Hello\n[00:03.00]World";
        let enc = base64::engine::general_purpose::STANDARD.encode(lrc.as_bytes());
        let dl: Value = serde_json::from_str(&format!(r#"{{"content":"{enc}"}}"#)).unwrap();
        let req = track("Test Song", "Test Artist", "Test Album", 200_000);
        let out = parse_kugou_response(&dl, req.duration_ms).expect("case A");
        assert_eq!(out.source, "kugou");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, 1000);
        assert_eq!(out.lines[0].duration_ms, 2000);
        assert_eq!(out.lines[0].text, "Hello");
        assert_eq!(out.lines[1].start_ms, 3000);
        assert_eq!(out.lines[1].duration_ms, 197_000);
        assert_eq!(out.lines[1].text, "World");
    }

    /// Parity case B: malformed base64 (`content` already plain LRC) keeps the raw string
    /// (:778-781 `try/except`). `[`/`]`/`:` are not base64, so decode fails -> fallback.
    #[test]
    fn malformed_base64_keeps_raw() {
        let dl: Value =
            serde_json::from_str(r#"{"content":"[00:05.00]Raw line"}"#).unwrap();
        let out = parse_kugou_response(&dl, 0).unwrap();
        assert_eq!(out.lines.len(), 1);
        assert_eq!(out.lines[0].start_ms, 5000);
        assert_eq!(out.lines[0].text, "Raw line");
    }

    /// Parity case C: empty `content` -> no lines -> "kugou: no lyrics".
    #[test]
    fn empty_content_yields_err() {
        let dl: Value = serde_json::from_str(r#"{"content":""}"#).unwrap();
        let err = parse_kugou_response(&dl, 0).unwrap_err();
        assert_eq!(err, "kugou: no lyrics");
    }

    /// `value_str` preserves a numeric `id` and a base64 `accesskey` for the download URL.
    #[test]
    fn value_str_preserves_id_and_accesskey() {
        let v: Value = serde_json::from_str(r#"{"id":123456,"accesskey":"AB+C/="}"#).unwrap();
        assert_eq!(value_str(&v, "id"), "123456");
        assert_eq!(value_str(&v, "accesskey"), "AB+C/=");
    }

    /// Live Kugou smoke test — disabled by default (needs network).
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "Michael Jackson", "", 257_000);
        let _ = fetch_kugou(&req).expect("live kugou fetch");
    }
}
