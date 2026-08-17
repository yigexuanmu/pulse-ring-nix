//! Web wallpaper via Electron running as a Wayland layer-shell surface.
//!
//! F-wl-paper architecture: pulse-ring spawns `wl-paper` (from the wl-proxy
//! project), which wraps Electron as a `zwlr_layer_surface_v1` on the compositor.
//! Electron renders the folia wallpaper directly to its OWN GPU surface — there
//! is NO stdout frame pipe and NO wgpu texture upload on the pulse-ring side.
//! The compositor composites the folia layer surface above the pulse-ring
//! background layer (Layer::Background) which still draws the ring + image
//! wallpaper; folia uses Layer::Bottom so it sits one step above the ring.
//!
//! stdin pipe is preserved: pulse-ring pushes config/lyrics/playback/theme/audio
//! to Electron via the same tag protocol. `wl-paper`'s
//! `spawn_and_forward_exit_code` inherits all three std handles, so the stdin
//! pipe flows pulse-ring → wl-paper → electron unchanged. stdout is set to null
//! (electron/main.js no longer emit RGBA frames — main.js logs to stderr only).

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Legacy frame type kept so `drain_web` stays callable from the render loop
/// during the transition. F-wl-paper produces no frames — `rx` is a permanently
/// empty channel, so `drain_web` always returns `None` and the wgpu overlay pass
/// stays a no-op (no texture to upload).
pub struct WebFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct WebWallpaperPlayer {
    pub rx: Receiver<WebFrame>,
    /// Child stdin: we push audio frames (and the manifest config) to the page.
    /// This is the stdin of the `wl-paper` process; wl-paper inherits it and
    /// `spawn_and_forward_exit_code` passes the same fd to the electron child.
    stdin: Option<std::process::ChildStdin>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    child: Option<Child>,
}

impl WebWallpaperPlayer {
    /// Send one audio frame: tag 0x00 + 128 f32 bands + 1 f32 energy (1+516 bytes, LE).
    /// The tag keeps the stream in sync with the Electron helper's parser.
    pub fn send_audio(&mut self, bands: &[f32; 128], energy: f32) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        use std::io::Write;
        let mut buf = Vec::with_capacity(517);
        buf.push(0u8); // tag: audio frame
        for b in bands.iter().take(128) {
            buf.extend_from_slice(&b.to_le_bytes());
        }
        buf.extend_from_slice(&energy.to_le_bytes());
        let _ = stdin.write_all(&buf);
    }

    /// Send the wallpaper manifest (JSON) to the page via `window.pulseRing.onConfig`.
    pub fn send_config(&mut self, config_json: &str) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        use std::io::Write;
        let bytes = config_json.as_bytes();
        let mut buf = Vec::with_capacity(4 + bytes.len());
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
        // Frame type tag: 0 = audio, 1 = config.
        let _ = stdin.write_all(&[1u8]);
        let _ = stdin.write_all(&buf);
    }

    /// Send resolved lyrics (JSON) to the page via `window.pulseRing.onLyrics`.
    /// Frame type tag: 2 (length-prefixed JSON, same envelope as config).
    pub fn send_lyrics(&mut self, json: &str) {
        self.send_tagged_json(2u8, json);
    }

    /// Send playback state (JSON) to the page via `window.pulseRing.onPlayback`.
    /// Frame type tag: 3 (length-prefixed JSON).
    pub fn send_playback(&mut self, json: &str) {
        self.send_tagged_json(3u8, json);
    }

    /// Send visualizer theme (JSON) to the page via `window.pulseRing.onTheme`.
    /// Frame type tag: 4 (length-prefixed JSON).
    pub fn send_theme(&mut self, json: &str) {
        self.send_tagged_json(4u8, json);
    }

    fn send_tagged_json(&mut self, tag: u8, json: &str) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        use std::io::Write;
        let bytes = json.as_bytes();
        let mut buf = Vec::with_capacity(4 + bytes.len());
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
        let _ = stdin.write_all(&[tag]);
        let _ = stdin.write_all(&buf);
    }
}

impl Drop for WebWallpaperPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Resolve the Electron binary used to run the wallpaper helper.
///
/// Priority:
///   1. `PULSE_RING_ELECTRON` env var (set by the Nix wrapper to `pkgs.electron`).
///      This is mandatory for an *installed* binary, where `CARGO_MANIFEST_DIR`
///      is a stale build-time source path that no longer exists at runtime.
///   2. The npm-installed Electron pinned by this project
///      (`electron-wallpaper/node_modules/.bin/electron`). Used when running via
///      `cargo run` from the source tree after `npm install` in electron-wallpaper.
///      Keeps a pinned Chromium/Node ABI for developers who want exact control.
fn electron_binary() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("PULSE_RING_ELECTRON") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("electron-wallpaper/node_modules/.bin/electron")
}

/// Resolve the `wl-paper` binary (Wayland proxy that wraps arbitrary clients as
/// layer-shell surfaces). Priority mirrors electron_binary(): env var first
/// (set by the Nix wrapper to the wl-proxy build's `bin/wl-paper`), then a
/// source-tree fallback for `cargo run` development.
fn wl_paper_binary() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("PULSE_RING_WL_PAPER") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wl-paper")
}

/// Start the folia wallpaper as a Wayland layer-shell surface.
///
/// Spawn chain:
///   pulse-ring
///     └── wl-paper --layer bottom --keyboard-interactivity none
///           └── electron --no-sandbox <main.js>   (stdin/stdout/stderr inherited)
///
/// `wl-paper` translates Electron's `xdg_toplevel` into a `zwlr_layer_surface_v1`
/// and forwards all std handles to the electron child, so the existing stdin tag
/// protocol (config/lyrics/playback/theme/audio) keeps working unchanged.
/// `width`/`height` are advisory: the compositor configures the layer surface to
/// the output's full size, so Electron's initial BrowserWindow size is mostly
/// cosmetic (it gets resized by the first configure).
pub fn start_web_wallpaper(html_path: &str, width: u32, height: u32) -> Result<WebWallpaperPlayer, String> {
    let wl_paper = wl_paper_binary();
    if !wl_paper.is_file() {
        return Err(format!(
            "wl-paper not found at {} (set PULSE_RING_WL_PAPER or build the wl-proxy flake input)",
            wl_paper.display()
        ));
    }
    let electron = electron_binary();
    if !electron.is_file() {
        return Err(format!(
            "Electron not found at {} (set PULSE_RING_ELECTRON to an electron binary, or run npm install in electron-wallpaper)",
            electron.display()
        ));
    }
    let helper: String = if let Ok(p) = std::env::var("PULSE_RING_HELPER") {
        p
    } else {
        concat!(env!("CARGO_MANIFEST_DIR"), "/electron-wallpaper/main.js").to_string()
    };
    // 远程 URL (如 folia OBS browser source /obs?obs=1&token=...) 直接透传, 不做
    // abs 路径转换 — URL 不是文件系统路径, 走 BrowserWindow.loadURL 而非 loadFile.
    let abs_html = if is_url_path(html_path) {
        html_path.to_string()
    } else if std::path::Path::new(html_path).is_absolute() {
        html_path.to_string()
    } else {
        format!(
            "{}/{}",
            std::env::current_dir().map_err(|e| e.to_string())?.display(),
            html_path
        )
    };

    let electron_str = electron.to_string_lossy().into_owned();
    // wl-paper: --layer bottom sits ABOVE the pulse-ring Layer::Background surface
    // (which draws the ring + image wallpaper) so folia's decorated lyrics render
    // over the ring while the transparent body lets the ring show through. Earlier
    // iterations kept folia as a wgpu overlay texture inside the background surface
    // (Pass 1.5), which meant a 960x540 -> fullscreen bilinear upscale and the
    // blurriness you saw; giving folia its own full-res layer surface fixes that.
    // --keyboard-interactivity none keeps the layer from stealing keyboard focus
    // (folia lyrics are display-only; pulse-ring keeps its own input handling).
    //
    // trailing_var_arg: everything after the wl-paper options is the electron
    // command line (electron --no-sandbox <main.js>) and is passed verbatim to
    // `Command::new(electron).args(["--no-sandbox", main.js])` by wl-paper's
    // `spawn_and_forward_exit_code`. htmlPath / width / height still go via env
    // (PULSE_RING_HTML / WIDTH / HEIGHT) to keep main.js's argv parsing untouched
    // and avoid wl-paper's clap trying to interpret them.
    let mut child = Command::new(&wl_paper)
        .arg("--layer")
        .arg("bottom")
        .arg("--keyboard-interactivity")
        .arg("none")
        .arg(&electron_str)
        .arg("--no-sandbox")
        .arg(&helper)                       // main script: wl-paper → electron 的 argv[1]
        .env("PULSE_RING_HTML", &abs_html)   // htmlPath via env (main.js 仍读)
        .env("PULSE_RING_WIDTH", width.to_string())
        .env("PULSE_RING_HEIGHT", height.to_string())
        .stdin(Stdio::piped())               // pulse-ring → wl-paper → electron (inherit)
        .stdout(Stdio::null())               // electron 不再发射 RGBA 帧; 丢弃任何意外 stdout
        .stderr(Stdio::inherit())            // electron console.error + wl-paper env_logger → 这里
        .spawn()
        .map_err(|e| format!("spawn wl-paper failed: {e}"))?;

    let stdin = child.stdin.take();
    // F-wl-paper: electron renders to its own layer surface, no frame stream on
    // stdout. Keep a permanently-empty channel so `drain_web` (called from the
    // render loop) stays a no-op without touching its call sites in main.rs.
    let (_tx, rx) = channel::<WebFrame>();
    let stop = Arc::new(AtomicBool::new(false));

    // No reader thread in F-wl-paper. `handle` stays None; Drop joins nothing.
    Ok(WebWallpaperPlayer {
        rx,
        stdin,
        stop,
        handle: None,
        child: Some(child),
    })
}

/// Drain the newest web wallpaper frame (drop stale ones).
pub fn drain_web(rx: &Receiver<WebFrame>) -> Option<WebFrame> {
    let mut newest = None;
    loop {
        match rx.try_recv() {
            Ok(f) => newest = Some(f),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    newest
}

/// True when a wallpaper path is an HTML file (web wallpaper).
pub fn is_html_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".html") || path.to_ascii_lowercase().ends_with(".htm")
}

/// True when a wallpaper target is a remote http(s) URL
/// (e.g. `http://127.0.0.1:32108/obs?obs=1&token=...` — folia OBS browser source).
/// URL targets skip local-pack resolution; the BrowserWindow calls `loadURL`
/// instead of `loadFile`, and the remote page (folia) drives its own data via
/// SSE from its backend, leaving pulse-ring only to capture frames.
pub fn is_url_path(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}
