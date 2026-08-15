//! Apple Music adapter — port of `lyric_sources.py::fetch_apple_music`.
//!
//! Searches `api.music.apple.com/v1/catalog/{storefront}/search` for songs, picks the best
//! match via the shared `best_match` (attr.name / attr.artistName / attr.albumName), then
//! fetches the song's lyrics (`amp-api…/songs/{id}/lyrics`) as raw text. The lyrics endpoint
//! usually returns a JSON envelope whose `ttml`/`data.attributes.ttml` holds the body — that
//! is fed to `parse_payload`; otherwise we fall back to recursing the envelope with
//! `parse_json_lines`. Non-JSON bodies (a bare TTML response) go straight to `parse_payload`.
//!
//! Credentials: `APPLE_DEVELOPER_TOKEN` (Bearer; required), optional `APPLE_USER_TOKEN`
//! (`Music-User-Token`); storefront via `APPLE_STOREFRONT` (default `us`, validated ASCII).

use crate::lyrics::{LyricData, TrackRequest};
use serde_json::Value;
use super::jsonparse::{first_value, first_value_str, parse_json_lines, parse_payload};
use super::{best_match, json_str, query_url, request_data, request_json, REQUEST_TIMEOUT};

pub(crate) fn fetch_apple_music(req: &TrackRequest) -> Result<LyricData, String> {
    let dev = std::env::var("APPLE_DEVELOPER_TOKEN")
        .map_err(|_| "apple_music: developer token required".to_string())?;
    let storefront = std::env::var("APPLE_STOREFRONT").unwrap_or_else(|_| "us".to_string());
    if storefront.is_empty()
        || !storefront
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("apple_music: invalid storefront".to_string());
    }

    let auth = format!("Bearer {}", dev);
    let mut headers: Vec<(String, String)> = vec![
        ("Authorization".to_string(), auth),
        ("Origin".to_string(), "https://music.apple.com".to_string()),
    ];
    if let Ok(ut) = std::env::var("APPLE_USER_TOKEN") {
        if !ut.is_empty() {
            headers.push(("Music-User-Token".to_string(), ut));
        }
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let search_url = query_url(
        &format!("https://api.music.apple.com/v1/catalog/{storefront}/search"),
        &[
            ("term", format!("{} {}", req.title, req.artist)),
            ("types", "songs".to_string()),
            ("limit", "10".to_string()),
        ],
    );
    let search = request_json(&search_url, Some(&header_refs), REQUEST_TIMEOUT)?;
    let songs_arr = first_value(&search, &[&["results", "songs", "data"]])
        .and_then(|v| v.as_array())
        .ok_or("apple_music: no results")?;
    let song_items: Vec<&Value> = songs_arr.iter().collect();
    let best_idx = best_match(
        &song_items,
        &req.title,
        &req.artist,
        &req.album,
        |x: &Value| json_str(&x["attributes"], "name"),
        |x: &Value| json_str(&x["attributes"], "artistName"),
        |x: &Value| json_str(&x["attributes"], "albumName"),
    )
    .ok_or("apple_music: no match")?;
    let best: &Value = song_items[best_idx];
    let best_id = best["id"].as_str().ok_or("apple_music: no id")?;

    let lyrics_url = format!(
        "https://amp-api.music.apple.com/v1/catalog/{storefront}/songs/{best_id}/lyrics"
    );
    let (body, _cs) = request_data(&lyrics_url, Some(&header_refs), REQUEST_TIMEOUT)?;
    let text = String::from_utf8_lossy(&body);
    let lines = match serde_json::from_str::<Value>(&text) {
        Ok(payload) => {
            let lyric_data = first_value_str(
                &payload,
                &[
                    &["ttml"],
                    &["data", "attributes", "ttml"],
                    &["data", "attributes", "content"],
                ],
            );
            if !lyric_data.is_empty() {
                parse_payload(&lyric_data)
            } else {
                parse_json_lines(&payload)
            }
        }
        Err(_) => parse_payload(&text),
    };

    Ok(LyricData {
        source: "apple_music".to_string(),
        lines,
    })
}
