//! Musixmatch adapter — port of `lyric_sources.py::fetch_musixmatch`.
//!
//! Hits `apic-desktop.musixmatch.com/ws/1.1/macro.subtitles.get` with the desktop app id +
//! a user token, pulls `macro_calls.subtitle.subtitle_body` (an LRC string) and parses it
//! via the LRC parser (or `parse_payload` if it's not timestamped). When the response also
//! carries `macro_calls.track.translation_list.translations`, those translated rows are
//! merged into the primary lines' `translation` field by nearest start time (tol 2s)
//! (inline `merge_timed`).
//!
//! Credentials: `MUSIXMATCH_USERTOKEN` (required).

use crate::lyrics::{LyricData, LyricLine, TrackRequest};
use super::jsonparse::{first_value, first_value_str, parse_payload};
use super::lrc::parse_lrc;
use super::{json_num, json_str, query_url, request_json, REQUEST_TIMEOUT};

pub(crate) fn fetch_musixmatch(req: &TrackRequest) -> Result<LyricData, String> {
    let token = std::env::var("MUSIXMATCH_USERTOKEN")
        .map_err(|_| "musixmatch: usertoken required".to_string())?;
    let params: Vec<(&str, String)> = vec![
        ("app_id", "web-desktop-app-v1.0".to_string()),
        ("usertoken", token),
        ("q_track", req.title.clone()),
        ("q_artist", req.artist.clone()),
        ("q_album", req.album.clone()),
        ("subtitle_format", "lrc".to_string()),
        ("page_size", "5".to_string()),
    ];
    let url = query_url(
        "https://apic-desktop.musixmatch.com/ws/1.1/macro.subtitles.get",
        &params,
    );
    let header_refs: &[(&str, &str)] = &[
        ("Origin", "https://www.musixmatch.com"),
        ("Referer", "https://www.musixmatch.com/"),
    ];
    let data = request_json(&url, Some(header_refs), REQUEST_TIMEOUT)?;

    let subtitle = first_value_str(
        &data,
        &[
            &["macro_calls", "subtitle", "subtitle_body"],
            &["macro_calls", "track", "subtitle_body"],
        ],
    );
    if subtitle.is_empty() {
        return Err("musixmatch: lyrics unavailable".to_string());
    }
    let mut lines = if subtitle.contains('[') {
        parse_lrc(&subtitle)
    } else {
        parse_payload(&subtitle)
    };

    if let Some(translations) = first_value(
        &data,
        &[
            &["macro_calls", "track", "translation_list", "translations"],
            &["macro_calls", "subtitle", "translation_list", "translations"],
        ],
    ) {
        if let Some(arr) = translations.as_array() {
            let mut tlines: Vec<LyricLine> = Vec::new();
            for item in arr {
                let t = json_str(item, "translation");
                if t.is_empty() {
                    continue;
                }
                let start = json_num(item.get("time"), 0);
                tlines.push(LyricLine {
                    start_ms: start,
                    duration_ms: 0,
                    text: t,
                    translation: String::new(),
                    romanization: String::new(),
                    chars: Vec::new(),
                    words: Vec::new(),
                    song_part: String::new(),
                    block_index: 0,
                    chorus_flag: false,
                });
            }
            if !lines.is_empty() && !tlines.is_empty() {
                let timed: Vec<&LyricLine> = tlines.iter().filter(|l| l.start_ms >= 0).collect();
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
    }

    Ok(LyricData {
        source: "musixmatch".to_string(),
        lines,
    })
}
