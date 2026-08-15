//! Bridge: serialize pulse-ring's lyrics / playback / theme state into the JSON
//! shapes folia's `window.pulseRing` expects, then push them to the Electron
//! offscreen web wallpaper via `WebWallpaperPlayer::send_*`.
//!
//! These run on the app's main thread (same place `send_audio` is called), so
//! there is no extra locking: a player is borrowed mutably, serialized inline,
//! and pushed across a single stdin write.

use crate::lyrics::{LyricData, LyricLine};
use crate::web_wallpaper::WebWallpaperPlayer;

/// Serialize resolved lyrics into folia's `PulseRingLyricData` shape:
///   { lines: [{ startTime, endTime, fullText, words: [{startTime,endTime,text}] }], offset }
///
/// A line's `endTime` is the next line's start (or start+3s for the last line),
/// so folia's `findLatestActiveLineIndex` keeps a line "active" until the next
/// begins — matching how pulse-ring's own lyric widget highlights.
pub fn lyrics_json(data: &LyricData) -> String {
    let mut s = String::from("{\"lines\":[");
    let n = data.lines.len();
    for (i, line) in data.lines.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let end = if i + 1 < n {
            data.lines[i + 1].time
        } else {
            // Last line: hold 3s (instrumental tail) so it stays active.
            line.time + 3.0
        };
        s.push_str("{\"startTime\":");
        s.push_str(&fmt_f32(line.time));
        s.push_str(",\"endTime\":");
        s.push_str(&fmt_f32(end));
        s.push_str(",\"fullText\":");
        s.push_str(&json_str(&line.text));
        // Per-word timeline: pulse-ring's words are (start_offset, end_offset, text)
        // relative to the line start; folia wants absolute startTime/endTime.
        s.push_str(",\"words\":[");
        let wn = line.words.len();
        for (j, (wstart, wend, wtext)) in line.words.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            s.push_str("{\"startTime\":");
            s.push_str(&fmt_f32(line.time + wstart));
            s.push_str(",\"endTime\":");
            s.push_str(&fmt_f32(line.time + wend));
            s.push_str(",\"text\":");
            s.push_str(&json_str(wtext));
            s.push('}');
        }
        s.push(']');
        s.push('}');
        let _ = wn; // (wn used only for the loop bound above)
    }
    s.push_str("],\"offset\":");
    s.push_str(&fmt_f32(data.offset));
    s.push('}');
    s
}

/// Serialize playback into folia's `PulseRingPlayback` shape:
///   { positionSec, durationSec, playing, title, artist, album, coverUrl, seed }
pub fn playback_json(
    position_sec: f32,
    duration_sec: f32,
    playing: bool,
    title: &str,
    artist: &str,
    album: &str,
    cover_url: Option<&str>,
) -> String {
    let seed = if title.is_empty() { String::from("pulse-ring") } else { title.to_string() };
    format!(
        "{{\"positionSec\":{},\"durationSec\":{},\"playing\":{},\"title\":{},\"artist\":{},\"album\":{},\"coverUrl\":{},\"seed\":{}}}",
        fmt_f32(position_sec.max(0.0)),
        fmt_f32(duration_sec.max(0.0)),
        playing,
        json_str(title),
        json_str(artist),
        json_str(album),
        cover_url.map_or("null".to_string(), |u| json_str(u)),
        json_str(&seed),
    )
}

/// Serialize a minimal folia `Theme` derived from pulse-ring's ring palette.
/// folia visualizers only read backgroundColor / primaryColor / accentColor /
/// secondaryColor + fontStyle + animationIntensity, so we map:
///   - backgroundColor: a darkened version of the first ring color (or near-black)
///   - primaryColor / accentColor / secondaryColor: ring colors 0/1/2
///   - animationIntensity: derived from sensitivity
pub fn theme_json(
    ring_colors: &[[f32; 4]],
    sensitivity: f32,
) -> String {
    let bg = dark_color(ring_colors.first().copied().unwrap_or([0.04, 0.03, 0.07, 1.0]));
    let primary = rgba_hex(ring_colors.first().copied().unwrap_or([0.92, 0.87, 1.0, 1.0]));
    let accent = rgba_hex(ring_colors.get(1).copied().unwrap_or([1.0, 0.84, 0.25, 1.0]));
    let secondary = rgba_hex(ring_colors.get(2).copied().unwrap_or([0.72, 0.70, 0.78, 1.0]));
    let intensity = if sensitivity > 1.5 { "chaotic" } else if sensitivity < 0.6 { "calm" } else { "normal" };
    format!(
        "{{\"name\":\"pulse-ring\",\"backgroundColor\":{},\"primaryColor\":{},\"accentColor\":{},\"secondaryColor\":{},\"fontStyle\":\"sans\",\"animationIntensity\":\"{}\"}}",
        json_str(&bg), json_str(&primary), json_str(&accent), json_str(&secondary), intensity,
    )
}

/// Push lyrics to a player (if any). Cheap no-op when None.
pub fn send_lyrics(player: Option<&mut WebWallpaperPlayer>, data: &LyricData) {
    if let Some(p) = player {
        p.send_lyrics(&lyrics_json(data));
    }
}

pub fn send_playback(
    player: Option<&mut WebWallpaperPlayer>,
    position_sec: f32,
    duration_sec: f32,
    playing: bool,
    title: &str,
    artist: &str,
    album: &str,
    cover_url: Option<&str>,
) {
    if let Some(p) = player {
        p.send_playback(&playback_json(
            position_sec, duration_sec, playing, title, artist, album, cover_url,
        ));
    }
}

pub fn send_theme(player: Option<&mut WebWallpaperPlayer>, ring_colors: &[[f32; 4]], sensitivity: f32) {
    if let Some(p) = player {
        p.send_theme(&theme_json(ring_colors, sensitivity));
    }
}

// ---- helpers ----

fn fmt_f32(v: f32) -> String {
    if v.is_finite() {
        // Trim trailing zeros for compactness; keep 3 decimals.
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        "0".to_string()
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// [f32;4] RGBA (0..1) → "#RRGGBB".
fn rgba_hex(c: [f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

/// Darken an RGBA color to a near-black background tint (preserve a hint of hue).
fn dark_color(c: [f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 40.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 40.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 40.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{LyricData, LyricLine};
    use serde_json::Value;

    fn parse(s: &str) -> serde_json::Result<Value> {
        serde_json::from_str::<Value>(s)
    }

    #[test]
    fn lyrics_two_lines_shape() {
        let data = LyricData {
            lines: vec![
                LyricLine {
                    time: 10.0,
                    text: "first line".to_string(),
                    words: vec![(0.5, 1.0, "hello".to_string())],
                },
                LyricLine {
                    time: 20.0,
                    text: "second".to_string(),
                    words: vec![],
                },
            ],
            offset: 0.0,
        };
        let json = parse(&lyrics_json(&data)).expect("valid json");
        let lines = json.get("lines").and_then(|l| l.as_array());
        assert_eq!(lines.unwrap().len(), 2, "two lines");

        let l0 = &json["lines"][0];
        assert_eq!(l0["startTime"], 10.0, "line0 startTime");
        // Line endTime = next line start (NOT start+3s, since there IS a next line).
        assert_eq!(l0["endTime"], 20.0, "line0 endTime = next line start");
        assert_eq!(l0["fullText"], "first line");
        let w0 = &l0["words"][0];
        // CRITICAL: word startTime must be ABSOLUTE (line.time + word offset), not relative.
        assert_eq!(w0["startTime"], 10.5, "word startTime absolute (10+0.5)");
        assert_eq!(w0["endTime"], 11.0, "word endTime absolute (10+1.0)");
        assert_eq!(w0["text"], "hello");

        let l1 = &json["lines"][1];
        assert_eq!(l1["startTime"], 20.0);
        assert_eq!(l1["endTime"], 23.0, "last line endTime = start+3s");
        assert_eq!(l1["words"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn playback_all_fields_and_cover_url() {
        let j = parse(&playback_json(12.5, 180.0, true, "T", "A", "Al", Some("file:///x.jpg")))
            .unwrap();
        assert_eq!(j["positionSec"], 12.5);
        assert_eq!(j["durationSec"], 180.0);
        assert_eq!(j["playing"], true);
        assert_eq!(j["title"], "T");
        assert_eq!(j["artist"], "A");
        assert_eq!(j["album"], "Al");
        assert_eq!(j["coverUrl"], "file:///x.jpg", "Some cover -> URL string");
        assert_eq!(j["seed"], "T", "seed = title when non-empty");
    }

    #[test]
    fn playback_no_cover_is_null_and_default_seed() {
        let j = parse(&playback_json(-1.0, 0.0, false, "", "", "", None)).unwrap();
        // Negative position clamped to 0.
        assert_eq!(j["positionSec"], 0.0);
        assert_eq!(j["durationSec"], 0.0);
        assert_eq!(j["playing"], false);
        assert_eq!(j["coverUrl"], Value::Null, "None cover -> null");
        assert_eq!(j["seed"], "pulse-ring", "empty title -> default seed");
    }

    #[test]
    fn theme_colors_hex_and_intensity() {
        // sensitivity maps: 0.3->calm, 1.0->normal, 2.0->chaotic
        // Ring color RED: primary -> #FF0000; backgroundColor (darken*40/255) -> near-black.
        let calm = parse(&theme_json(&[[1.0, 0.0, 0.0, 1.0]], 0.3)).unwrap();
        assert_eq!(calm["animationIntensity"], "calm");
        assert_eq!(calm["primaryColor"], "#FF0000");
        // backgroundColor = color*40/255 -> (40,0,0) -> #280000
        assert_eq!(calm["backgroundColor"], "#280000");
        assert!(calm["accentColor"].as_str().unwrap().starts_with('#'));
        assert!(calm["secondaryColor"].as_str().unwrap().starts_with('#'));

        let normal = parse(&theme_json(&[[1.0, 1.0, 1.0, 1.0]], 1.0)).unwrap();
        assert_eq!(normal["animationIntensity"], "normal");
        assert_eq!(normal["primaryColor"], "#FFFFFF");

        let chaotic = parse(&theme_json(&[], 2.0)).unwrap();
        assert_eq!(chaotic["animationIntensity"], "chaotic");
        // No ring colors -> defaults (non-empty hex strings).
        assert!(chaotic["primaryColor"].as_str().unwrap().starts_with('#'));
    }

    #[test]
    fn empty_lyrics() {
        let data = LyricData { lines: vec![], offset: 0.0 };
        let j = parse(&lyrics_json(&data)).expect("valid json");
        assert_eq!(j["lines"].as_array().unwrap().len(), 0);
        assert_eq!(j["offset"], 0.0);
    }

    #[test]
    fn playback_title_escaping() {
        // A title with a double-quote and a newline must round-trip through JSON.
        let orig = "a\"b\nc"; // a"b<newline>c
        let j = parse(&playback_json(0.0, 0.0, false, orig, "", "", None)).unwrap();
        let back = j["title"].as_str().unwrap();
        assert_eq!(back, orig, "quoted/newline must survive JSON round-trip");
    }

    #[test]
    fn lyrics_text_escaping() {
        let data = LyricData {
            lines: vec![LyricLine {
                time: 0.0,
                text: "quote\"and\\slash".to_string(),
                words: vec![],
            }],
            offset: 0.0,
        };
        let j = parse(&lyrics_json(&data)).unwrap();
        assert_eq!(j["lines"][0]["fullText"], "quote\"and\\slash");
    }
}
