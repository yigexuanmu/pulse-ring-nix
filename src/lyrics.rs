//! Lyric fetching via the bundled `lyric_sources.py` CLI.
//!
//! The Python script is a pure-stdlib multi-source lyric adapter (LRCLIB / Netease / QQ /
//! Kugou / SPlayer / ...). We spawn it as a subprocess with a request file (read+deleted by
//! the script so credentials never touch disk), then normalise its unified line model into
//! [`LyricData`]. All times are millisecond integers, matching the adapter's model.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;

/// Pure-Rust lyric fetch layer (replaces the `python3 lyric_sources.py` subprocess). The
/// source adapters are filled in incrementally; until wired in, [`fetch_lyrics`] still
/// drives the legacy Python path so the project keeps building and rendering.
#[path = "lyricfetch/mod.rs"]
mod lyricfetch;

/// One lyric line in the unified model (times in ms).
#[derive(Debug, Clone)]
pub struct LyricLine {
    /// Line start time in ms. Lines without a timeline are filtered out on ingest.
    pub start_ms: i64,
    /// Line duration in ms (auto-inferred by the adapter when missing).
    pub duration_ms: i64,
    /// Original lyric text.
    pub text: String,
    /// Translation (may be empty).
    pub translation: String,
    /// Romanisation / pinyin (may be empty).
    pub romanization: String,
    /// Per-character timestamps in ms (used for sub-word reveal; may be empty).
    pub chars: Vec<i64>,
}

/// Parsed lyric set for one track.
#[derive(Debug, Clone)]
pub struct LyricData {
    pub source: String,
    pub lines: Vec<LyricLine>,
}

impl LyricData {
    /// Total duration covered by the last line (ms), or 0.
    pub fn end_ms(&self) -> i64 {
        self.lines.last().map(|l| l.start_ms + l.duration_ms).unwrap_or(0)
    }
}

/// A track change request sent to the fetch worker.
#[derive(Debug, Clone)]
pub struct TrackRequest {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Track duration in ms (0 if unknown).
    pub duration_ms: i64,
    /// Source to query ("auto" → adapter chain; otherwise a specific source id).
    pub source: String,
    /// Optional per-track lyric URL (used by the `ttml` source).
    pub ttml_url: String,
}

impl TrackRequest {
    /// Stable identity used to detect track changes (skips album/source).
    pub fn key(&self) -> String {
        format!("{}|{}", self.title, self.artist)
    }
}

/// Locate `lyric_sources.py`: `$PULSE_RING_LYRIC_SCRIPT`, then a few known install paths.
pub fn resolve_script() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PULSE_RING_LYRIC_SCRIPT") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        PathBuf::from("/usr/share/pulse-ring/lyrics/lyric_sources.py"),
        PathBuf::from("/usr/local/share/pulse-ring/lyrics/lyric_sources.py"),
        PathBuf::from("./lyrics/lyric_sources.py"),
        PathBuf::from(&home).join(".config/pulse-ring/lyrics/lyric_sources.py"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Run the adapter for a single request and parse its stdout JSON into [`LyricData`].
pub fn fetch_lyrics(script: &PathBuf, request: &TrackRequest) -> Result<LyricData, String> {
    // Prefer the native in-process Rust adapter (clean-room port of lyric_sources.py).
    // On Err (source not ported / network / parse), fall back to the python3 subprocess.
    if let Ok(data) = lyricfetch::fetch(request) {
        return Ok(data);
    }
    let payload = serde_json::json!({
        "source": request.source,
        "track": {
            "title": request.title,
            "artist": request.artist,
            "album": request.album,
            "duration": request.duration_ms * 1000, // script treats >10e6 as microseconds
            "ttml_url": request.ttml_url,
        },
        "credentials": {},
        "options": {},
    });
    let req_path = std::env::temp_dir().join(format!("pulse-ring-lyric-{}.json", std::process::id()));
    std::fs::File::create(&req_path)
        .and_then(|mut f| f.write_all(payload.to_string().as_bytes()))
        .map_err(|e| format!("cannot write request file: {e}"))?;

    let output = Command::new("python3")
        .arg(script)
        .arg(&req_path)
        .output()
        .map_err(|e| format!("failed to run python3: {e}"))?;
    // The script deletes the request file itself; clean up in case it never ran.
    let _ = std::fs::remove_file(&req_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("lyric_sources.py exited with {:?}: {}", output.status.code(), stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("bad lyric JSON: {e}"))?;
    parse_response(&value)
}

/// Convert the adapter's unified JSON response into [`LyricData`].
fn parse_response(value: &serde_json::Value) -> Result<LyricData, String> {
    let obj = value.as_object().ok_or("response is not an object")?;
    let ltype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("none");
    if ltype != "lyrics" {
        let diag = obj.get("diag").and_then(|v| v.as_array()).map(|a| {
            a.iter().filter_map(|d| d.as_str()).collect::<Vec<_>>().join("; ")
        }).unwrap_or_default();
        return Err(if diag.is_empty() { "no lyrics matched".into() } else { diag });
    }
    let source = obj.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let lines_arr = obj.get("lines").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut lines = Vec::with_capacity(lines_arr.len());
    for item in &lines_arr {
        let time = item.get("time").and_then(|v| v.as_i64()).unwrap_or(-1);
        if time < 0 {
            continue; // untimed (plain) lines can't drive animations yet
        }
        let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if text.trim().is_empty() {
            continue;
        }
        let duration = item.get("duration").and_then(|v| v.as_i64()).unwrap_or(0);
        let translation = item.get("translation").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let romanization = item.get("romanization").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let chars = item.get("chars")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|c| c.as_i64()).collect())
            .unwrap_or_default();
        lines.push(LyricLine {
            start_ms: time,
            duration_ms: duration.max(0),
            text,
            translation,
            romanization,
            chars,
        });
    }
    if lines.is_empty() {
        return Err("lyrics response had no timed lines".into());
    }
    Ok(LyricData { source, lines })
}

/// A background fetch worker. Sends [`TrackRequest`]s through a channel; results come back on
/// [`LyricRx`] in the same order they were processed (latest request wins, older ones are
/// dropped). Each network fetch may take up to ~20s (per-adapter timeouts).
pub struct LyricWorker {
    pub tx: mpsc::Sender<TrackRequest>,
    pub rx: mpsc::Receiver<Result<LyricData, String>>,
}

impl LyricWorker {
    pub fn spawn() -> Self {
        let (req_tx, req_rx) = mpsc::channel::<TrackRequest>();
        let (res_tx, res_rx) = mpsc::channel::<Result<LyricData, String>>();
        std::thread::Builder::new()
            .name("pulse-ring-lyrics".into())
            .spawn(move || {
                let script = resolve_script();
                if script.is_none() {
                    log::warn!("lyric_sources.py not found; lyrics disabled");
                }
                loop {
                    // Wait for the first request, then drain the queue keeping only the
                    // newest one (stale track changes are dropped).
                    let req = match req_rx.recv() {
                        Ok(first) => first,
                        Err(_) => return, // sender dropped
                    };
                    let mut latest = req;
                    while let Ok(next) = req_rx.try_recv() {
                        latest = next;
                    }
                    // Primary source, then fall back to lrclib when it has nothing.
                    let mut result = match &script {
                        Some(p) => fetch_lyrics(p, &latest),
                        None => Err("lyric script missing".into()),
                    };
                    if result.is_err() && latest.source != "lrclib" {
                        let fallback = TrackRequest { source: "lrclib".to_string(), ..latest.clone() };
                        result = match &script {
                            Some(p) => fetch_lyrics(p, &fallback),
                            None => result,
                        };
                    }
                    if res_tx.send(result).is_err() {
                        return;
                    }
                }
            })
            .expect("spawn lyric worker");
        LyricWorker { tx: req_tx, rx: res_rx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adapter_response() {
        let json = r#"{
            "type": "lyrics",
            "source": "lrclib",
            "lines": [
                {"time": 1200, "duration": 1800, "text": "hello", "translation": "你好", "chars": [1200, 1500]},
                {"time": 3000, "duration": 900, "text": "world"},
                {"time": -1, "text": "untimed should be dropped"}
            ]
        }"#;
        let data = parse_response(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(data.source, "lrclib");
        assert_eq!(data.lines.len(), 2);
        assert_eq!(data.lines[0].text, "hello");
        assert_eq!(data.lines[0].translation, "你好");
        assert_eq!(data.lines[0].chars, vec![1200, 1500]);
        assert_eq!(data.lines[1].start_ms, 3000);
        assert_eq!(data.end_ms(), 3900);
    }

    #[test]
    fn rejects_empty_or_none_responses() {
        let none = r#"{"type": "none", "source": "lrclib", "lines": [], "diag": ["lrclib: no match"]}"#;
        assert!(parse_response(&serde_json::from_str(none).unwrap()).is_err());

        let no_timed = r#"{"type": "lyrics", "source": "lrclib", "lines": [{"time": -1, "text": "x"}]}"#;
        assert!(parse_response(&serde_json::from_str(no_timed).unwrap()).is_err());
    }

    #[test]
    fn track_request_key_is_stable() {
        let a = TrackRequest { title: "Song".into(), artist: "Artist".into(), album: "A".into(), duration_ms: 1, source: "lrclib".into(), ttml_url: String::new() };
        let b = TrackRequest { title: "Song".into(), artist: "Artist".into(), album: "B".into(), duration_ms: 2, source: "netease".into(), ttml_url: "x".into() };
        assert_eq!(a.key(), b.key());
    }
}
