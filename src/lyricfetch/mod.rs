//! Native Rust lyric fetch layer — clean-room port of the bundled `lyric_sources.py`.
//!
//! Replaces the `python3 lyric_sources.py` subprocess (`lyrics.rs` used to spawn) with
//! in-process HTTP + format parsers. The public entry is [`fetch`], which dispatches by
//! [`TrackRequest::source`] to a chain of source adapters (LRCLIB / NetEase / QQ / Kugou /
//! SPlayer / QiShai / TTML / Spotify / Apple / Musixmatch), each producing a [`LyricData`]
//! directly — no JSON intermediate. Shared text/number helpers mirror the top-of-file
//! utilities in `lyric_sources.py` (cited per helper).

use crate::lyrics::{LyricData, TrackRequest};

pub(crate) mod jsonparse;
pub(crate) mod lrc;
pub(crate) mod lrclib;
pub(crate) mod netease;
pub(crate) mod qqmusic;
pub(crate) mod splayer;
pub(crate) mod kugou;
pub(crate) mod qishui;
pub(crate) mod ttml;
pub(crate) mod spotify;
pub(crate) mod apple_music;
pub(crate) mod musixmatch;

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
        "lrclib" => lrclib::fetch_lrclib(req),
        "netease" | "netease_public" => netease::fetch_netease(req),
        "qqmusic" => qqmusic::fetch_qqmusic(req),
        "splayer" => splayer::fetch_splayer(req),
        "kugou" => kugou::fetch_kugou(req),
        "qishui" => qishui::fetch_qishui(req),
        "ttml" => ttml::fetch_ttml(req),
        "spotify" => spotify::fetch_spotify(req),
        "apple_music" => apple_music::fetch_apple_music(req),
        "musixmatch" => musixmatch::fetch_musixmatch(req),
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

// ---- shared HTTP helpers (mirror lyric_sources.py module utils, lines 415-435) ----

use std::time::Duration;

/// Noctalia UA sent on every adapter request (lyric_sources.py:18).
const USER_AGENT: &str = "Noctalia-Lyrics/1.0";

/// Default per-adapter network timeout (lyric_sources.py:415 `timeout=15`).
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// `form_urlencoded`-style percent encoding mirroring `urllib.parse.quote_plus` (used by
/// `query_url`, lyric_sources.py:434): unreserved `A-Za-z0-9-_.~` stay, space -> `+`,
/// everything else -> `%XX` of the UTF-8 byte (uppercase hex).
fn percent_encode_form(value: &str) -> String {
    use std::fmt::Write;
    let mut buf = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                buf.push(*byte as char);
            }
            b' ' => buf.push('+'),
            b => {
                buf.push('%');
                let _ = write!(buf, "{b:02X}");
            }
        }
    }
    buf
}

/// `query_url` (lyric_sources.py:434): join `base` with `?` (or `&` if already queried) and
/// the form-encoded `params`; insertion order preserved (keys here are static ASCII literals).
pub(crate) fn query_url(base: &str, params: &[(&str, String)]) -> String {
    let sep = if base.contains('?') { '&' } else { '?' };
    let mut out = String::from(base);
    out.push(sep);
    let mut first = true;
    for (k, v) in params {
        if !first {
            out.push('&');
        }
        first = false;
        out.push_str(k);
        out.push('=');
        out.push_str(&percent_encode_form(v));
    }
    out
}

/// `request_json` (lyric_sources.py:426): GET with the Noctalia UA + JSON accept headers
/// (plus any caller-supplied headers, e.g. NetEase's `Referer`), decode the body (stripping
/// a `callback(...)` JSONP wrapper if present), parse JSON. POST form bodies land with the
/// QQ/Kugou sources; this is the GET profile.
pub(crate) fn request_json(
    url: &str,
    headers: Option<&[(&str, &str)]>,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let mut req = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json, text/plain, application/xml, text/xml");
    if let Some(extra) = headers {
        for (k, v) in extra {
            req = req.set(k, v);
        }
    }
    let resp = req
        .timeout(timeout)
        .call()
        .map_err(|e| format!("request_json: {e}"))?;
    let body = resp
        .into_string()
        .map_err(|e| format!("request_json: read body: {e}"))?;
    let text = body.trim();
    let payload = if text.starts_with("callback(") && text.ends_with(')') {
        &text["callback(".len()..text.len() - 1]
    } else {
        text
    };
    serde_json::from_str(payload).map_err(|e| format!("request_json: bad JSON: {e}"))
}

/// `request_data` (lyric_sources.py:415): GET the raw body bytes (NOT parsed as JSON). QiShui
/// + TTML use this to pull LRC / plain-text / TTML payloads that aren't JSON. Mirrors
/// `request_json`'s Noctalia-UA + timeout profile but swaps JSON accept headers for a
/// text-friendly Accept and returns the body bytes + the response charset (decoded UTF-8:
/// `Response::into_string` already lossy-converts to UTF-8 using the declared charset, so the
/// returned bytes are UTF-8 regardless; the charset is surfaced for callers that want to
/// re-tag the body — currently ignored by QiShui).
pub(crate) fn request_data(
    url: &str,
    headers: Option<&[(&str, &str)]>,
    timeout: Duration,
) -> Result<(Vec<u8>, String), String> {
    let mut req = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "text/plain, application/xml, text/xml, */*");
    if let Some(extra) = headers {
        for (k, v) in extra {
            req = req.set(k, v);
        }
    }
    let resp = req
        .timeout(timeout)
        .call()
        .map_err(|e| format!("request_data: {e}"))?;
    // Sniff the charset off Content-Type BEFORE consuming `resp` with `into_string` (the
    // borrow over the Content-Type header ends at `.to_string()`, freeing resp below).
    let charset = resp
        .header("Content-Type")
        .and_then(|ct| ct.split("charset=").nth(1))
        .map(|s| {
            s.trim()
                .split(|c| c == ';' || c == ' ')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_else(|| "utf-8".to_string());
    let body = resp
        .into_string()
        .map_err(|e| format!("request_data: read body: {e}"))?
        .into_bytes();
    Ok((body, charset))
}

/// `best_match` (lyric_sources.py:457-469): pick the item (by index) scoring highest against
/// the wanted track — title exact=6/substr=3 (a zero skips the item), artist 4/2, album 2/1.
/// Qualifying matches need >=3; ties keep the earliest item (Python's strict-greater min).
pub(crate) fn best_match<T, TF, AF, LF>(
    items: &[&T],
    wanted_title: &str,
    wanted_artist: &str,
    wanted_album: &str,
    title: TF,
    artist: AF,
    album: LF,
) -> Option<usize>
where
    TF: Fn(&T) -> String,
    AF: Fn(&T) -> String,
    LF: Fn(&T) -> String,
{
    let w_title = normalize(wanted_title);
    let w_artist = normalize(wanted_artist);
    let w_album = normalize(wanted_album);
    let mut best: Option<(usize, i64)> = None;
    for (index, item) in items.iter().enumerate() {
        let t = normalize(&title(item));
        let a = normalize(&artist(item));
        let al = normalize(&album(item));
        let mut score: i64 = 0;
        if !w_title.is_empty() && !t.is_empty() {
            let t_score = if t == w_title {
                6
            } else if w_title.contains(&t) || t.contains(&w_title) {
                3
            } else {
                0
            };
            if t_score == 0 {
                continue;
            }
            score += t_score;
        }
        if !w_artist.is_empty() && !a.is_empty() {
            score += if a == w_artist {
                4
            } else if w_artist.contains(&a) || a.contains(&w_artist) {
                2
            } else {
                0
            };
        }
        if !w_album.is_empty() && !al.is_empty() {
            score += if al == w_album {
                2
            } else if w_album.contains(&al) || al.contains(&w_album) {
                1
            } else {
                0
            };
        }
        match best {
            Some((_, bs)) if bs >= score => {}
            _ => best = Some((index, score)),
        }
    }
    best.and_then(|(i, s)| if s >= 3 { Some(i) } else { None })
}

/// Read a JSON object field as a cleaned string, tolerating numbers/bools — parity with
/// Python `clean_text(item.get(k))`.
pub(crate) fn json_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => clean_text(s),
        Some(other) => clean_text(&other.to_string()),
        None => String::new(),
    }
}

/// Python `number(value, default)` over a JSON value: a JSON number (int/float) or a numeric
/// string; otherwise `default`.
pub(crate) fn json_num(v: Option<&serde_json::Value>, default: i64) -> i64 {
    match v {
        Some(serde_json::Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(default),
        Some(serde_json::Value::String(s)) => number(s, default),
        _ => default,
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
