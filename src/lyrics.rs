//! In-process Rust lyric fetching.
//!
//! The bundled `lyricfetch` module is a pure-Rust multi-source lyric adapter (LRCLIB /
//! Netease / QQ / Kugou / SPlayer / ...) running in-process — no Python subprocess.
//! [`fetch_lyrics`] hands a [`TrackRequest`] straight to that module, which produces a
//! [`LyricData`] with all times as millisecond integers.

use std::sync::mpsc;

/// Pure-Rust lyric fetch layer. Source adapters live in the in-process `lyricfetch` module;
/// [`fetch_lyrics`] forwards a [`TrackRequest`] to [`lyricfetch::fetch`].
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

/// Fetch lyrics for one request via the in-process `lyricfetch` adapter chain.
pub fn fetch_lyrics(request: &TrackRequest) -> Result<LyricData, String> {
    lyricfetch::fetch(request)
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
                    let mut result = fetch_lyrics(&latest);
                    if result.is_err() && latest.source != "lrclib" {
                        let fallback = TrackRequest { source: "lrclib".to_string(), ..latest.clone() };
                        result = fetch_lyrics(&fallback);
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
    fn track_request_key_is_stable() {
        let a = TrackRequest { title: "Song".into(), artist: "Artist".into(), album: "A".into(), duration_ms: 1, source: "lrclib".into(), ttml_url: String::new() };
        let b = TrackRequest { title: "Song".into(), artist: "Artist".into(), album: "B".into(), duration_ms: 2, source: "netease".into(), ttml_url: "x".into() };
        assert_eq!(a.key(), b.key());
    }
}
