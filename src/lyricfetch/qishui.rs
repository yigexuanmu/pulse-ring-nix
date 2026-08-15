//! QiShui source adapter — clean-room port of `lyric_sources.py::adapter_qishui` (lines
//! 782-805). Unlike the catalog sources (LRCLIB/QQ/Kugou), QiShui is a *template* endpoint:
//! the operator exposes a single URL with `{title}`/`{artist}`/`{album}` placeholders and
//! QiShui resolves the track server-side, returning a LRC (or plain-text) body directly —
//! no search/resolve round-trip. The endpoint URL is supplied via `QISHUI_API_URL` and an
//! optional bearer token via `QISHUI_TOKEN` (lyric_sources.py:784-786). We percent-encode
//! each substitution (path-style `%XX`, unreserved set only), validate the rendered URL is
//! http(s) with a host, then GET it and parse the body with the bracket auto-detector
//! (`parse_lrc` if `[` is present, else `parse_plain`). Lines are `finalize`-d against the
//! track duration — the same double-finalize Python applies via `success(...)`.
//!
//! Parity scope notes:
//! - Deviation from `lyric_sources.py`: the Python `request(...)` helper (lines 415-435)
//!   returns the raw body + charset; this codebase exposes only `request_json` (which parses
//!   JSON). QiShui replies LRC/plain text, so we keep a local raw-body GET (`get_text`) that
//!   mirrors `request_json`'s Noctalia-UA + timeout profile but skips the JSON decode.
//! - URL validation is done with a manual scheme+host check rather than the `url` crate,
//!   to avoid introducing a new dependency (equivalent to `urllib.parse.urlparse`'s
//!   scheme/`netloc` presence check).
//! - QiShui has no cover-art concept in `crate::lyrics::LyricData` (it carries only
//!   `source` + `lines`), matching the LRCLIB/Kugou adapters' treatment of cover fields.

use std::time::Duration;

use super::lrc::{parse_lrc, parse_plain};
use super::{finalize, REQUEST_TIMEOUT};
use crate::lyrics::{LyricData, TrackRequest};

/// Adapter entry: render the QiShui template URL and fetch lyrics for `req`.
/// Mirrors `adapter_qishui` (lyric_sources.py:782-805).
pub(crate) fn fetch_qishui(req: &TrackRequest) -> Result<LyricData, String> {
    let endpoint = std::env::var("QISHUI_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "qishui: endpoint required".to_string())?; // :784

    // Substitute the three placeholders with path-style percent-encoded values (:785-790).
    let url = endpoint
        .replace("{title}", &url_encode(&req.title))
        .replace("{artist}", &url_encode(&req.artist))
        .replace("{album}", &url_encode(&req.album));

    // Validate the rendered URL is http(s) with a host (:791-793).
    validate_http_url(&url)?;

    // Optional bearer token from `QISHUI_TOKEN`, trimmed (:794-795).
    let token = std::env::var("QISHUI_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty());

    // GET the endpoint with the Noctalia UA (and bearer if present). Mirrors request_json's
    // header/timeout profile but returns the raw body text (QiShui replies LRC, not JSON).
    let text = get_text(
        &url,
        token.as_deref(),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("qishui: {e}"))?; // :796-799

    parse_qishui_body(&text, req.duration_ms)
}

/// HTTP-free selection + parse — the testable core of `adapter_qishui`. `text` is the raw
/// body from the rendered template URL; `duration_ms` drives the final `finalize` (the
/// success-double-finalize that Python's `success(...)` applies).
fn parse_qishui_body(text: &str, duration_ms: i64) -> Result<LyricData, String> {
    // Bracket auto-detector: `[` present -> LRC, else plain, else empty (lyric_sources.py
    // :800-802 `parse_payload`).
    let parsed = if text.contains('[') {
        parse_lrc(text)
    } else if !text.trim().is_empty() {
        parse_plain(text)
    } else {
        Vec::new()
    };
    let lines = finalize(parsed, duration_ms); // success(...) -> finalize(lines, duration)
    if lines.is_empty() {
        return Err("qishui: no lyrics".to_string()); // success([]) -> empty(...)
    }
    Ok(LyricData {
        source: "qishui".to_string(),
        lines,
    })
}

/// `urllib.parse.quote`-style percent encoding for URL path segments (:787-790): unreserved
/// `A-Za-z0-9-_.~` stay, every other byte -> uppercase `%XX` (space is `%20`, not `+`, since
/// these values substitute into the path, not a form query). Differs from the shared
/// `percent_encode_form` (query strings, space -> `+`).
fn url_encode(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            let _ = write!(out, "{b:02X}");
        }
    }
    out
}

/// Validate the rendered URL has an `http`/`https` scheme and a non-empty host
/// (lyric_sources.py:791-793 `urllib.parse.urlparse(...).scheme in {http,https}` + netloc).
/// Manual check avoids pulling the `url` crate; equivalent to `urlparse`'s presence test.
fn validate_http_url(url: &str) -> Result<(), String> {
    let after = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| "qishui: invalid endpoint".to_string())?;
    let host_end = after
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(after.len());
    let host = &after[..host_end];
    if host.is_empty() {
        return Err("qishui: invalid endpoint".to_string());
    }
    Ok(())
}

/// Raw-body GET — mirrors `request_json` (lyric_sources.py:426) but skips JSON parsing and
/// returns the body as a UTF-8-lossy string (ureq's `into_string` is already lossy). The
/// optional `bearer` token is sent as `Authorization: Bearer <token>` (:794-795).
fn get_text(url: &str, bearer: Option<&str>, timeout: Duration) -> Result<String, String> {
    let mut req = ureq::get(url)
        .set("User-Agent", "Noctalia-Lyrics/1.0")
        .set("Accept", "text/plain, application/xml, text/xml, */*");
    if let Some(token) = bearer {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    let resp = req
        .timeout(timeout)
        .call()
        .map_err(|e| format!("request: {e}"))?;
    resp.into_string()
        .map_err(|e| format!("read body: {e}"))
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
            source: "qishui".to_string(),
            ttml_url: String::new(),
        }
    }

    /// Parity case A: LRC body with `[` present parses to timed lines, finalized against the
    /// track duration. Ground truth (adapter_qishui over this body) in /tmp/parity_qishui.py:
    /// `[1000/2000/Hello]`, `[3000/197000/World]`.
    #[test]
    fn lrc_body_parses() {
        let body = "[00:01.00]Hello\n[00:03.00]World";
        let req = track("Test Song", "Test Artist", "Test Album", 200_000);
        let out = parse_qishui_body(body, req.duration_ms).expect("case A");
        assert_eq!(out.source, "qishui");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, 1000);
        assert_eq!(out.lines[0].duration_ms, 2000);
        assert_eq!(out.lines[0].text, "Hello");
        assert_eq!(out.lines[1].start_ms, 3000);
        assert_eq!(out.lines[1].duration_ms, 197_000);
        assert_eq!(out.lines[1].text, "World");
    }

    /// Parity case B: plain body (no `[`) parses to untimed lines via `parse_plain`.
    /// Ground truth: `[{-1/0/Line one},{-1/0/Line two}]` (captured in /tmp/parity_qishui2.py).
    #[test]
    fn plain_body_parses() {
        let body = "Line one\nLine two";
        let out = parse_qishui_body(body, 200_000).expect("case B");
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].start_ms, -1);
        assert_eq!(out.lines[0].duration_ms, 0);
        assert_eq!(out.lines[0].text, "Line one");
        assert_eq!(out.lines[1].start_ms, -1);
        assert_eq!(out.lines[1].duration_ms, 0);
        assert_eq!(out.lines[1].text, "Line two");
    }

    /// Parity case C: empty body -> "qishui: no lyrics" (success([]) -> empty(...)).
    #[test]
    fn empty_body_yields_err() {
        let err = parse_qishui_body("", 0).unwrap_err();
        assert_eq!(err, "qishui: no lyrics");
    }

    /// Parity case C': whitespace-only body is trimmed-empty -> "qishui: no lyrics".
    #[test]
    fn whitespace_body_yields_err() {
        let err = parse_qishui_body("   \n  ", 0).unwrap_err();
        assert_eq!(err, "qishui: no lyrics");
    }

    /// `url_encode` keeps the unreserved set, encodes space as `%20` (path-style, not `+`),
    /// and encodes non-ASCII bytes as uppercase `%XX` over the UTF-8 byte sequence.
    #[test]
    fn url_encode_unreserved_and_non_ascii() {
        assert_eq!(url_encode("A B-C_d.e~f"), "A%20B-C_d.e~f");
        assert_eq!(url_encode("中文"), "%E4%B8%AD%E6%96%87");
        assert_eq!(url_encode(""), "");
        assert_eq!(url_encode("AC/DC"), "AC%2FDC");
    }

    /// `validate_http_url` accepts http(s) URLs with a host and rejects other schemes /
    /// empty hosts / non-URLs.
    #[test]
    fn validate_http_url_accepts_only_http_with_host() {
        assert!(validate_http_url("https://api.example.com/lyrics?x=1").is_ok());
        assert!(validate_http_url("http://api.example.com/lyrics").is_ok());
        assert_eq!(
            validate_http_url("ftp://api.example.com/x").unwrap_err(),
            "qishui: invalid endpoint"
        );
        assert_eq!(
            validate_http_url("https:///path").unwrap_err(),
            "qishui: invalid endpoint"
        );
        assert_eq!(
            validate_http_url("not a url").unwrap_err(),
            "qishui: invalid endpoint"
        );
    }

    /// Live QiShui smoke test — disabled by default (needs network + `QISHUI_API_URL`).
    /// Run with `cargo test -- --ignored lyricfetch::qishui::tests::live_smoke`.
    #[test]
    #[ignore]
    fn live_smoke() {
        let req = track("Bad", "Michael Jackson", "Bad", 257_000);
        let _ = fetch_qishui(&req).expect("live qishui fetch");
    }
}
