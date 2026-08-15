//! Spotify adapter — port of `lyric_sources.py::fetch_spotify`.
//!
//! Searches `api.spotify.com/v1/search` for tracks, picks the best match via the shared
//! `best_match` (title/artist/album), then pulls synced lyrics from the colour-lyrics v2
//! endpoint and recurses them through the shared `parse_json_lines`. If the response carries
//! a translated `alternatives[0].lines`, the nearest-time lines are merged into the primary
//! set's `translation` field (inline `merge_timed`, tol 2s).
//!
//! Credentials: `SPOTIFY_ACCESS_TOKEN` (or `SPOTIFY_SP_DC`) → `Authorization: Bearer …`.

use crate::lyrics::{LyricData, LyricLine, TrackRequest};
use serde_json::Value;
use super::jsonparse::{first_value, parse_json_lines};
use super::{best_match, json_str, query_url, request_json, REQUEST_TIMEOUT};

pub(crate) fn fetch_spotify(req: &TrackRequest) -> Result<LyricData, String> {
    let token = std::env::var("SPOTIFY_ACCESS_TOKEN")
        .or_else(|_| std::env::var("SPOTIFY_SP_DC"))
        .map_err(|_| "spotify: credentials required".to_string())?;
    let auth = format!("Bearer {}", token);
    let header_refs: &[(&str, &str)] = &[("Authorization", auth.as_str())];

    let search_url = query_url(
        "https://api.spotify.com/v1/search",
        &[
            ("q", format!("{} {}", req.title, req.artist)),
            ("type", "track".to_string()),
            ("limit", "10".to_string()),
        ],
    );
    let search = request_json(&search_url, Some(header_refs), REQUEST_TIMEOUT)?;
    let items = search["tracks"]["items"]
        .as_array()
        .ok_or("spotify: no results")?;
    let track_items: Vec<&Value> = items.iter().collect();
    let best_idx = best_match(
        &track_items,
        &req.title,
        &req.artist,
        &req.album,
        |x: &Value| json_str(x, "name"),
        |x: &Value| {
            let mut parts = Vec::new();
            if let Some(arr) = x["artists"].as_array() {
                for a in arr {
                    parts.push(json_str(a, "name"));
                }
            }
            parts.join(" ")
        },
        |x: &Value| json_str(&x["album"], "name"),
    )
    .ok_or("spotify: no match")?;
    let best: &Value = track_items[best_idx];
    let track_id = best["id"].as_str().ok_or("spotify: no track id")?;

    let mut lyrics_url = format!(
        "https://spclient.wg.spotify.com/color-lyrics/v2/track/{}",
        track_id
    );
    lyrics_url = query_url(
        &lyrics_url,
        &[
            ("format", "json".to_string()),
            ("market", "from_token".to_string()),
        ],
    );
    let data = request_json(&lyrics_url, Some(header_refs), REQUEST_TIMEOUT)?;
    let mut lines = parse_json_lines(&data["lyrics"]["lines"]);

    // Spotify "alternatives" → merge as nearest-time translations.
    if let Some(alt) = first_value(&data, &[&["lyrics", "alternatives", "0", "lines"]]) {
        let alt_lines = parse_json_lines(alt);
        if !lines.is_empty() && !alt_lines.is_empty() {
            let timed: Vec<&LyricLine> = alt_lines.iter().filter(|l| l.start_ms >= 0).collect();
            let tol: i64 = 2000;
            for target in lines.iter_mut() {
                if target.start_ms < 0 || timed.is_empty() {
                    continue;
                }
                let mut best = 0usize;
                let mut diff = (target.start_ms - timed[0].start_ms).abs();
                for (i, c) in timed.iter().enumerate().skip(1) {
                    let d = (target.start_ms - c.start_ms).abs();
                    if d < diff {
                        diff = d;
                        best = i;
                    }
                }
                if diff <= tol && target.translation.is_empty() {
                    target.translation = timed[best].text.clone();
                }
            }
        }
    }

    Ok(LyricData {
        source: "spotify".to_string(),
        lines,
    })
}
