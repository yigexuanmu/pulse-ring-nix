use std::num::NonZeroU32;
use std::ptr::NonNull;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
    shell::WaylandSurface,
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
    Connection, Proxy, QueueHandle,
};

mod audio;
mod config;
mod draw;
mod lua;
mod lyrics;
mod lyricstyles;
mod lyricview;
mod plugin;
mod preview;
mod sdf;
use audio::NBANDS;
use draw::RingRenderer;

const MAX_PARTICLES: usize = 96;
const PARTICLE_STRIDE: usize = 12;

/// One full rendering instance per output (layer surface + wgpu surface + renderer).
struct OutputSurfaces {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    renderer: RingRenderer,
    width: u32,
    height: u32,
    configured: bool,
    closed: bool,
    frame_skip: u32,
}

struct App {
    compositor: CompositorState,
    layer_shell: LayerShell,
    start: std::time::Instant,
    registry_state: RegistryState,
    output_state: OutputState,
    cfg: config::Config,
    bands: [f32; NBANDS],
    audio_rx: crossbeam_channel::Receiver<[f32; NBANDS]>,
    /// wgpu instance/device/queue shared across all outputs.
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    display_handle: RawDisplayHandle,
    outputs: Vec<OutputSurfaces>,
    image_cache: Vec<(String, std::sync::Arc<ImageData>)>,
    font: std::sync::Arc<rusttype::Font<'static>>,
    // Per-widget clock cache: (last_text, tex_w, tex_h, tex_index)
    clock_cache: [(String, u32, u32, u32); 8],
    texture_slots: Vec<Option<ImageData>>,
    /// Per-slot dirty flag: true when the slot's ImageData content changed this frame
    /// and needs re-upload to any renderer that hasn't seen this version yet.
    /// Set at every texture_slots write site; cleared by each renderer after it uploads.
    texture_slot_dirty: Vec<bool>,
    widget_uvs: [(f32, f32, f32, f32); 32],
    cover_rx: std::sync::mpsc::Receiver<ImageData>,
    last_cover_path: String,
    cover_tex_index: usize,
    cover_loaded: bool,
    cover_aspect: f32,
    current_cover: Option<ImageData>,
    cover_uploaded: bool,
    cover_slot: usize,
    lua_state: lua::LuaState,
    plugins: Vec<plugin::LoadedPlugin>,
    /// Reused RGBA staging buffer for plugin renders (allocated once, 512x512x4 = 1MB).
    /// Previously this was `vec![0u8; 512*512*4]` every frame → 30MB/sec allocation
    /// → allocator fragmentation → freeze after long use.
    plugin_buf: Vec<u8>,
    plugin_smooth_bands: [f32; 128],
    music: lua::MusicInfo,
    ring_amp_smooth: f32,
    last_music_poll: f32,
    lyric_worker_tx: std::sync::mpsc::Sender<lyrics::TrackRequest>,
    lyric_rx: std::sync::mpsc::Receiver<Result<lyrics::LyricData, String>>,
    // EDT background worker (Phase B, docs/EDT_BG_DESIGN.md): main->worker SPSC request.
    edt_worker_tx: std::sync::mpsc::Sender<sdf::EdtRequest>,
    // EDT background worker: worker->main SPSC result (R8 bytes + GlyphInfo for upload).
    edt_rx: std::sync::mpsc::Receiver<sdf::EdtResult>,
    /// Parsed lyrics for the current track (None = no track / not matched yet).
    lyrics: Option<lyrics::LyricData>,
    /// Identity of the track whose lyrics are cached (title|artist).
    lyric_key: String,
    /// Playback position in microseconds, refreshed by a background thread.
    pos_us: std::sync::Arc<std::sync::atomic::AtomicI64>,
    /// Smoothed playback position in seconds (lyric time).
    pos_sec: f32,
    /// True after the first playback-position sync. Until then we snap pos_sec straight
    /// to the target on the first update_pos call so a song that's already in progress
    /// when we start doesn't get every prior lyric "played through" during the smoothing
    /// ramp.
    pos_synced: bool,
    /// Measured wall-clock delta of the previous frame (seconds), written by the main
    /// loop every iteration and read by `update_pos` so the lyric playback timeline keeps
    /// advancing correctly when a frame drops and the 33 ms tick budget is exceeded. The
    /// old code passed a hardcoded `0.033`, so on multi-ms spikes the lyric position slid
    /// behind the real song position (D.8 / G6).
    frame_dt: f32,
    /// SDF glyph atlas backing real-time lyric text.
    glyph_atlas: sdf::GlyphAtlas,
    /// Last time lyric diagnostics were logged (seconds since start).
    last_lyric_log: f32,
    /// Rolling FPS window: timestamps (seconds since start) of the last N frames.
    /// Used to log a smoothed frame rate without spamming the log every frame.
    fps_window: Vec<f32>,
    /// Last time the FPS line was printed (seconds since start).
    last_fps_log: f32,
    /// Total frames rendered since start (for monotonic average).
    total_frames: u64,
    /// Process start instant — used as the zero point for FPS window timestamps.
    start_time: std::time::Instant,
    /// Audio energy [bass, vocal, power] 0..1 for music-reactive lyric particles.
    lyric_audio: [f32; 3],
    /// Capture the first frame after startup to this path (PULSE_RING_CAPTURE=...).
    capture_path: Option<String>,
    capture_done: bool,
}

fn main() {
    env_logger::init();

    // `pulse-ring sonnet [true|false]` — enable/disable the sonnet lyric animation.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("sonnet") {
        config::run_sonnet_subcommand(&args[1..]);
        return;
    }
    // `pulse-ring preview "<text>" [style] [time]` — headless PNG preview of the lyric layer.
    if args.first().map(|s| s.as_str()) == Some("preview") {
        let text = args.get(1).cloned().unwrap_or_else(|| "Hello world 你好世界".to_string());
        let style = args.get(2).cloned().unwrap_or_else(|| "sonnet".to_string());
        let time = args.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(2.5);
        let path = args.get(4).cloned().unwrap_or_else(|| "/tmp/pulse-ring-preview.png".to_string());
        match preview::render(&text, &style, time, &path) {
            Ok(()) => println!("preview written to {path}"),
            Err(e) => eprintln!("preview failed: {e}"),
        }
        return;
    }

    let mut cfg = config::Config::load(&config::config_path());
    let audio_rx = audio::start_audio(cfg.sensitivity, cfg.decay);

    let conn = Connection::connect_to_env().expect("failed to connect to Wayland");
    let (globals, mut event_queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor =
        CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr layer shell is not available");

    // Initialise wgpu against this Wayland connection. Devices are shared by all surfaces.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(conn.backend().display_ptr() as *mut _).unwrap(),
    ));
    // A dummy surface on a scratch wl_surface so we can pick a compatible adapter; it is
    // immediately destroyed — the real surfaces are created per-output in new_output().
    let scratch_surface = compositor.create_surface(&qh);
    let scratch_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(scratch_surface.id().as_ptr() as *mut _).unwrap(),
    ));
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(raw_display_handle),
        raw_window_handle: scratch_handle,
    };
    let scratch_wgpu = unsafe { instance.create_surface_unsafe(target) }
        .expect("create scratch wgpu surface");

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&scratch_wgpu),
        ..Default::default()
    }))
    .expect("no suitable GPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
        .expect("failed to acquire wgpu device");

    // Drop the scratch surfaces; real ones are created in new_output().
    drop(scratch_wgpu);

    let lua_script = cfg.lua_script.clone();
    let lua_state = lua::LuaState::new(lua_script.as_deref(), &mut cfg);
    let lyric_worker = lyrics::LyricWorker::spawn();
    let pos_us: std::sync::Arc<std::sync::atomic::AtomicI64> =
        std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    {
        let pos_us = pos_us.clone();
        std::thread::Builder::new()
            .name("pulse-ring-pos".into())
            .spawn(move || loop {
                let v = std::process::Command::new("playerctl")
                    .args(["position"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
                    .map(|s| (s * 1_000_000.0) as i64);
                if let Some(v) = v {
                    pos_us.store(v, std::sync::atomic::Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            })
            .expect("spawn position thread");
    }
    let font_data = crate::load_font_data();
    // EDT background worker (Phase B): spawn before the App literal so the font handle
    // can be shared (moved into the worker) and reused as App.font. Placeholder EDT
    // body for now; the real rasterise + edt_signed path is a followup commit.
    let font = std::sync::Arc::new(load_font());
    let edt_worker = sdf::EdtWorker::spawn(font.clone());
    let mut app = App {
        compositor,
        layer_shell,
        start: std::time::Instant::now(),
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        cfg,
        bands: [0.0; NBANDS],
        audio_rx,
        instance,
        device,
        queue,
        adapter,
        display_handle: raw_display_handle,
        outputs: Vec::new(),
        image_cache: Vec::new(),
        font,
        clock_cache: std::array::from_fn(|_| (String::new(), 0, 0, 0)),
        texture_slots: vec![None; 64],
        texture_slot_dirty: vec![false; 64],
        widget_uvs: [(0.0, 0.0, 0.0, 0.0); 32],
        cover_rx: spawn_cover_thread(),
        last_cover_path: String::new(),
        cover_tex_index: 0,
        cover_loaded: false,
        cover_aspect: 1.0,
        current_cover: None,
        cover_uploaded: false,
        cover_slot: 0,
        lua_state,
        plugins: plugin::load_plugins_with_log(),
        plugin_buf: vec![0u8; 512 * 512 * 4],
        plugin_smooth_bands: [0.0; 128],
        music: lua::MusicInfo::default(),
        ring_amp_smooth: 0.0,
        last_music_poll: -10.0,
        lyric_worker_tx: lyric_worker.tx,
        lyric_rx: lyric_worker.rx,
        edt_worker_tx: edt_worker.tx,
        edt_rx: edt_worker.rx,
        lyrics: None,
        lyric_key: String::new(),
        pos_us,
        pos_sec: 0.0,
        pos_synced: false,
        frame_dt: 0.033,
        glyph_atlas: sdf::GlyphAtlas::new_with_weights(
            &font_data,
            {
                let bold = crate::load_font_data_bold();
                if bold.is_empty() { None } else { Some(bold) }
            }.as_deref(),
            {
                let black = crate::load_font_data_black();
                if black.is_empty() { None } else { Some(black) }
            }.as_deref(),
            {
                let light = crate::load_font_data_light();
                if light.is_empty() { None } else { Some(light) }
            }.as_deref(),
        ).expect("glyph atlas"),
        last_lyric_log: -10.0,
        lyric_audio: [0.0; 3],
        capture_path: std::env::var("PULSE_RING_CAPTURE").ok(),
        capture_done: false,
        fps_window: Vec::with_capacity(120),
        last_fps_log: -10.0,
        total_frames: 0,
        start_time: std::time::Instant::now(),
    };

    // Wait for the first configure (outputs sized) via blocking dispatch, then switch to a
    // timed render loop (~60 fps) so the compositor only recomposites on our updates.
    let interval = std::time::Duration::from_millis(16);
    while !app.outputs.iter().any(|o| o.width > 0) {
        event_queue.blocking_dispatch(&mut app).unwrap();
        if !app.outputs.is_empty() && app.outputs.iter().all(|o| o.closed) {
            return;
        }
    }
    // Measured frame delta: the main loop now tracks the real wall-clock gap between
    // consecutive iterations (including the sleep at the bottom) and hands it to the
    // renderer so `update_pos` advances the lyric timeline by the true elapsed time
    // instead of a fixed 0.033 — frame drops no longer leave the lyric curtain drifting.
    let mut last_frame = std::time::Instant::now();
    loop {
        let before = std::time::Instant::now();
        let dt = before.duration_since(last_frame).as_secs_f32().min(0.1);
        last_frame = before;
        app.frame_dt = dt;
        event_queue.dispatch_pending(&mut app).unwrap();
        app.tick();
        if !app.outputs.is_empty() && app.outputs.iter().all(|o| o.closed) {
            break;
        }
        let elapsed = before.elapsed();
        if elapsed < interval {
            std::thread::sleep(interval - elapsed);
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _f: i32) {}
    fn transform_changed(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _t: wl_output::Transform) {}
    fn surface_enter(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}

    fn frame(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _t: u32) {
        // Rendering is driven by the timed tick() loop (~15 fps); frame callbacks are only
        // used to keep the surface presented.
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _c: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.outputs.iter().any(|o| o.output == output) {
            return;
        }
        // Create a layer surface bound to this specific output.
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Background,
            Some("pulse-ring"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        layer.commit();

        // wgpu surface for this output's wl_surface.
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(layer.wl_surface().id().as_ptr() as *mut _).unwrap(),
        ));
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(self.display_handle),
            raw_window_handle,
        };
        let wgpu_surface = unsafe { self.instance.create_surface_unsafe(target) }
            .expect("create wgpu surface for output");

        let renderer = RingRenderer::new(
            self.device.clone(),
            self.queue.clone(),
            wgpu_surface,
            &self.adapter,
            &self.cfg,
            self.outputs.len() as u32,
        );

        log::info!("added surface for output {}", output.id());
        self.outputs.push(OutputSurfaces {
            output,
            layer,
            renderer,
            width: 0,
            height: 0,
            configured: false,
            closed: false,
            frame_skip: 0,
        });
    }

    fn update_output(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.output == output) {
            if let Some(info) = self.output_state.info(&output) {
                if let Some((w, h)) = info.logical_size {
                    o.width = w.max(0) as u32;
                    o.height = h.max(0) as u32;
                }
            }
        }
    }

    fn output_destroyed(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.outputs.retain(|o| o.output != output);
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _c: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.layer == *layer) {
            o.closed = true;
        }
    }

    fn configure(&mut self, _c: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _serial: u32) {
        if let Some(idx) = self.outputs.iter().position(|o| o.layer == *layer) {
            log::info!("configure for output idx={idx} size={:?}", configure.new_size);
            let cfg_new_size = configure.new_size;
            let o = &mut self.outputs[idx];
            if cfg_new_size.0 > 0 {
                o.width = cfg_new_size.0;
            }
            if cfg_new_size.1 > 0 {
                o.height = cfg_new_size.1;
            }
            let first = !o.configured;
            o.configured = true;
            // Only the configured render target gets an initial draw; other screens stay
            // blank (no buffer) so niri has nothing to composite for them.
            let is_target = self.cfg.render_screen < 0 || self.cfg.render_screen == idx as i32;
            if first && is_target {
                let _ = qh; self.draw_one(idx);
            }
        }
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers!(OutputState);
}

smithay_client_toolkit::delegate_dispatch2!(App);

/// Startup expansion scale 0..1 (may overshoot for elastic/back easings).
fn spawn_scale_for(cfg: &crate::config::Config, elapsed: f32) -> f32 {
    use crate::config::{SpawnEffect, SpawnEase};
    if matches!(cfg.spawn_effect, SpawnEffect::None) {
        return 1.0;
    }
    let dur = cfg.spawn_duration.max(1.0) / 1000.0;
    let t = (elapsed / dur).min(1.0);
    let e = match cfg.spawn_ease {
        SpawnEase::OutCubic => 1.0 - (1.0 - t).powi(3),
        SpawnEase::OutBack => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        SpawnEase::Elastic => {
            let c4 = std::f32::consts::PI * 2.5;
            if t == 0.0 { 0.0 } else if t == 1.0 { 1.0 } else {
                2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
        SpawnEase::Bounce => {
            let n1 = 7.5625;
            let d1 = 2.75;
            if t < 1.0 / d1 {
                n1 * t * t
            } else if t < 2.0 / d1 {
                let t = t - 1.5 / d1;
                n1 * t * t + 0.75
            } else if t < 2.5 / d1 {
                let t = t - 2.25 / d1;
                n1 * t * t + 0.9375
            } else {
                let t = t - 2.625 / d1;
                n1 * t * t + 0.984375
            }
        }
    };
    e.clamp(0.0, 1.35)
}

/// Compute per-frame particle layout: 32 slots x (x, y, size, alpha, r, g, b, a) in pixels.
fn compute_particles(
    cfg: &crate::config::Config,
    elapsed: f32,
    width: u32,
    height: u32,
    amp_avg: f32,
) -> [f32; MAX_PARTICLES * PARTICLE_STRIDE] {
    use crate::config::ParticleMode;
    let mut out = [0.0f32; MAX_PARTICLES * PARTICLE_STRIDE];
    let min_d = width.min(height) as f32;
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    for (slot, p) in cfg.particles.iter().take(MAX_PARTICLES).enumerate() {
        let o = slot * PARTICLE_STRIDE;
        let t = elapsed - p.delay;
        if t < 0.0 {
            continue;
        }
        let a0 = p.angle.to_radians();
        let (mut px, mut py, mut vx, mut vy, mut alpha, size0) = match cfg.particle_mode {
            ParticleMode::Burst => {
                let period = p.life.max(0.1);
                let phase = if cfg.particle_loop { t % period } else { t.min(period) };
                let fade = (1.0 - phase / period).max(0.0);
                // Drag-damped distance: d = v0 * (1 - e^(-drag*t)) / drag
                let drag = p.drag.clamp(0.0, 20.0);
                let dist = if drag > 0.001 {
                    p.speed * min_d * (1.0 - (-drag * phase).exp()) / drag
                } else {
                    p.speed * min_d * phase
                };
                // Gravity: 0.5 * g * t^2 along +y (edge fractions per s^2)
                let g = p.gravity * min_d;
                let grav = 0.5 * g * phase * phase;
                let wave_off = p.wave * min_d * (phase * 6.2832 * 1.5 + a0).sin();
                let wx = -a0.sin() * wave_off;
                let wy = a0.cos() * wave_off;
                (
                    cx + p.x * min_d + a0.cos() * dist + wx,
                    cy + p.y * min_d + a0.sin() * dist + grav + wy,
                    a0.cos() * p.speed * min_d * (if drag > 0.001 { (-drag * phase).exp() } else { 1.0 }),
                    a0.sin() * p.speed * min_d * (if drag > 0.001 { (-drag * phase).exp() } else { 1.0 }) + g * phase,
                    fade,
                    p.size * min_d,
                )
            }
            ParticleMode::Orbit => {
                let w = p.speed.to_radians();
                let th = a0 + w * t;
                let r = ((p.x * p.x + p.y * p.y).sqrt() * min_d).max(1.0);
                let dir = vec2_angle(th);
                (
                    cx + dir.0 * r,
                    cy + dir.1 * r,
                    -dir.1 * w * r,
                    dir.0 * w * r,
                    1.0,
                    p.size * min_d,
                )
            }
            ParticleMode::Ring => {
                // Orbit just outside the ring's *current* edge: the band swells and shrinks
                // with the music (mean band amplitude) plus a small fixed offset `x`, so the
                // particles always hug the ring without ever being swallowed by it.
                let w = p.speed.to_radians();
                let th = a0 + w * t;
                // Orbit follows the ring's outer edge through the low-passed amplitude, so the
                // band swells/settles smoothly and never twitches in and out.
                let r = ((cfg.base_radius + cfg.growth * amp_avg + cfg.halo_size * 0.5 + p.x) * min_d)
                    .max(2.0);
                let dir = vec2_angle(th);
                (
                    cx + dir.0 * r,
                    cy + dir.1 * r,
                    -dir.1 * w * r,
                    dir.0 * w * r,
                    1.0,
                    p.size * min_d,
                )
            }
            ParticleMode::None => continue,
        };
        // Ring-mode particles stay rock steady (a Saturn band reads as a band, not a flicker);
        // burst/orbit get fade-in, twinkle and size interpolation.
        let (alpha, size) = if cfg.particle_mode == ParticleMode::Ring {
            (1.0, size0)
        } else {
            let fade_in = if p.fade_in > 0.0 { (t / p.fade_in).min(1.0) } else { 1.0 };
            let tw = 1.0 - p.twinkle.clamp(0.0, 1.0) * 0.5 * (1.0 + (t * 12.0 + slot as f32 * 1.7).sin());
            let alpha = alpha * fade_in * tw;
            let size = size0 + (p.size_end * min_d - size0) * (1.0 - alpha).clamp(0.0, 1.0);
            (alpha, size)
        };
        out[o] = px;
        out[o + 1] = py;
        out[o + 2] = size.max(0.5);
        out[o + 3] = alpha;
        out[o + 4] = p.color[0];
        out[o + 5] = p.color[1];
        out[o + 6] = p.color[2];
        out[o + 7] = p.color[3] * alpha;
        out[o + 8] = p.spin_speed.to_radians() * t;
        out[o + 9] = vx;
        out[o + 10] = vy;
    }
    out
}

fn vec2_angle(a: f32) -> (f32, f32) {
    (a.cos(), a.sin())
}


/// Current time as "HH:MM" (system local time, no chrono dependency).
/// Current local time parts: (hour, minute, second, sub-second fraction).
pub fn main_now_hmsparts() -> (i32, i32, i32, f32) {
    now_hmsparts()
}

fn now_hmsparts() -> (i32, i32, i32, f32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = secs;
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    (tm.tm_hour, tm.tm_min, tm.tm_sec, now.subsec_nanos() as f32 / 1e9)
}

/// Poll the MPRIS cover via `playerctl` every 2s, decode it, send RGBA through a channel.
fn spawn_cover_thread() -> std::sync::mpsc::Receiver<ImageData> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            let art = std::process::Command::new("playerctl")
                .args(["metadata", "mpris:artUrl"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(url) = art {
                let path = url.strip_prefix("file://").map(str::to_string).unwrap_or_else(|| url.clone());
                if last.as_deref() != Some(&path) {
                    last = Some(path.clone());
                    log::info!("cover: new art {path}");
                    match load_image_path(&path) {
                        Some(img) => { log::info!("cover: decoded {}x{}", img.w, img.h); let _ = tx.send(img); }
                        None => log::warn!("cover: decode failed {path}"),
                    }
                }
            } else {
                log::warn!("cover: no artUrl");
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    rx
}

/// Decode a PNG or JPEG file into RGBA (scaled to fit 256 slot).
fn load_image_path(path: &str) -> Option<ImageData> {
    let expanded = path.replacen('~', &std::env::var("HOME").unwrap_or_default(), 1);
    let bytes = std::fs::read(&expanded).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let img = ImageData { w, h, rgba: rgba.into_raw() };
    Some(fit_slot(img))
}

/// Current local time as two lines: "HH:MM\nMM-DD" (libc localtime_r, system timezone).
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = now;
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!(
        "{:02}:{:02}\n{:02}-{:02}",
        tm.tm_hour, tm.tm_min, tm.tm_mon + 1, tm.tm_mday
    )
}


/// Simple RGBA image holder.
#[derive(Clone, Debug)]
pub(crate) struct ImageData {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Everything the lyric layer needs from the CPU per frame.
struct LyricFrame {
    enabled: bool,
    time: f32,
    words: Vec<[f32; 20]>,
    /// Post-processing values for the lyric layer: [blur, glitch, noise, contrast].
    fx: [f32; 9],
    /// The SDF atlas gained glyphs this frame and should be re-uploaded to the renderer.
    atlas_dirty: bool,
}

/// Virtual "♪" staff lines around the playhead for instrumental tracks without lyrics
/// (folia's virtual-staff mode). Lines every 8s with a 6s window.
fn virtual_staff(t: f32) -> lyrics::LyricData {
    let start = ((t - 20.0) / 8.0).floor() as i64 * 8;
    let mut lines = Vec::with_capacity(10);
    for i in 0..10 {
        lines.push(lyrics::LyricLine {
            start_ms: (start + i as i64 * 8) * 1000,
            duration_ms: 6000,
            text: "♪".to_string(),
            translation: String::new(),
            romanization: String::new(),
            chars: vec![],
            words: vec![],
            song_part: String::new(),
            block_index: 0,
            chorus_flag: false,
        });
    }
    lyrics::LyricData { source: "virtual-staff".to_string(), lines }
}

/// Load a system font for clock/lyric rendering (CJK-capable preferred).
pub(crate) fn load_font() -> rusttype::Font<'static> {    let data = load_font_data();
    font_from_bytes(data, true).unwrap_or_else(|| {
        panic!("no usable system font found (install fontconfig + a CJK font, or set PULSE_RING_FONT)")
    })
}

/// Resolve the font file bytes (CJK-capable preferred), via hard-coded paths then fontconfig.
pub(crate) fn load_font_data() -> Vec<u8> {
    let candidates = [
        "/usr/share/fonts/TTF/JetBrains-Maple-Mono-NF-XX-XX/JetBrainsMapleMono-Regular.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    for p in candidates {
        if let Ok(data) = std::fs::read(p) {
            if let Some(f) = font_from_bytes(data.clone(), p.ends_with(".ttc")) {
                if f.glyph('中').id().0 > 0 {
                    return data;
                }
            }
        }
    }
    // fontconfig fallback: resolve a CJK-capable font then a generic sans.
    for pattern in ["sans:lang=zh-cn", "Noto Sans CJK SC", "sans-serif", "sans", "mono"] {
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", pattern])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && s != "TrueType");
        if let Some(path) = out {
            if let Ok(data) = std::fs::read(&path) {
                if let Some(f) = font_from_bytes(data.clone(), path.ends_with(".ttc")) {
                    if f.glyph('中').id().0 > 0 || pattern.contains("sans") || pattern == "mono" {
                        log::info!("font via fc-match: {path}");
                        return data;
                    }
                }
            }
        }
    }
    Vec::new()
}

/// Resolve a bold font file (for the sonnet hero/semi-hero weight hierarchy). Returns empty
/// when no bold face is available (callers fall back to regular).
pub(crate) fn load_font_data_bold() -> Vec<u8> {
    for pattern in ["sans:lang=zh-cn:weight=bold", "sans:weight=bold", "Noto Sans CJK SC Bold"] {
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", pattern])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && s != "TrueType");
        if let Some(path) = out {
            if let Ok(data) = std::fs::read(&path) {
                if !data.is_empty() {
                    log::info!("bold font via fc-match: {path}");
                    return data;
                }
            }
        }
    }
    Vec::new()
}

/// Black (weight 900) face for sonnet hero words (folia hero/semi = 900).
pub(crate) fn load_font_data_black() -> Vec<u8> {
    for pattern in ["sans:lang=zh-cn:weight=black", "sans:weight=black", "Noto Sans CJK SC Black"] {
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", pattern])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && s != "TrueType");
        if let Some(path) = out {
            if let Ok(data) = std::fs::read(&path) {
                if !data.is_empty() {
                    log::info!("black font via fc-match: {path}");
                    return data;
                }
            }
        }
    }
    Vec::new()
}

/// Light (weight 300) face for sonnet decoration words (folia decoration = 300).
pub(crate) fn load_font_data_light() -> Vec<u8> {
    for pattern in ["sans:lang=zh-cn:weight=light", "sans:weight=light", "sans:weight=300", "Noto Sans CJK SC Light"] {
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", pattern])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && s != "TrueType");
        if let Some(path) = out {
            if let Ok(data) = std::fs::read(&path) {
                if !data.is_empty() {
                    log::info!("light font via fc-match: {path}");
                    return data;
                }
            }
        }
    }
    Vec::new()
}

fn font_from_bytes(data: Vec<u8>, is_ttc: bool) -> Option<rusttype::Font<'static>> {
    if is_ttc {
        for idx in 0..8 {
            if let Some(f) = rusttype::Font::try_from_vec_and_index(data.clone(), idx) {
                return Some(f);
            }
        }
        None
    } else {
        rusttype::Font::try_from_vec(data)
    }
}

/// Decode a PNG file to RGBA.
fn load_png(path: &str) -> Option<ImageData> {
    let data = std::fs::read(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).ok()?;
    let w = info.width;
    let h = info.height;
    let bytes = &buf[..info.buffer_size()];
    let (rgba, w, h) = match info.color_type {
        png::ColorType::Rgba => (bytes.to_vec(), w, h),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for c in bytes.chunks_exact(3) {
                out.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
            (out, w, h)
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(bytes.len() * 4);
            for &g in bytes {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            (out, w, h)
        }
        _ => return None,
    };
    Some(ImageData { w, h, rgba })
}

/// Scale an image down (bilinear-ish) to fit a 256x256 atlas slot, keeping aspect.
fn fit_slot(img: ImageData) -> ImageData {
    const MAX: u32 = 512;
    if img.w <= MAX && img.h <= MAX {
        return img;
    }
    let scale = (MAX as f32 / img.w as f32).min(MAX as f32 / img.h as f32);
    let nw = ((img.w as f32 * scale).floor() as u32).max(1);
    let nh = ((img.h as f32 * scale).floor() as u32).max(1);
    let mut out = ImageData { w: nw, h: nh, rgba: vec![0u8; (nw * nh * 4) as usize] };
    for y in 0..nh {
        for x in 0..nw {
            let sx = ((x as f32 + 0.5) / scale - 0.5).max(0.0) as usize;
            let sy = ((y as f32 + 0.5) / scale - 0.5).max(0.0) as usize;
            let si = (sy * img.w as usize + sx) * 4;
            let di = ((y * nw + x) * 4) as usize;
            out.rgba[di..di + 4].copy_from_slice(&img.rgba[si..si + 4]);
        }
    }
    out
}

/// Rasterise a text string (may contain '\n' lines) to RGBA at the given font size.
fn rasterize_text(font: &rusttype::Font, text: &str, size_pt: f32, color: [f32; 4]) -> ImageData {
    // pt -> px at 96 DPI (13.5pt = 18px)
    let size_px = size_pt * 96.0 / 72.0;
    let scale = rusttype::Scale { x: size_px, y: size_px };
    let v_metrics = font.v_metrics(scale);
    let line_h = (v_metrics.ascent - v_metrics.descent).ceil() as u32;
    let lines: Vec<&str> = text.split('\n').collect();
    let mut g_w = 1u32;
    for line in &lines {
        let w: u32 = font
            .layout(line, scale, rusttype::point(0.0, 0.0))
            .map(|g| g.unpositioned().h_metrics().advance_width.ceil() as u32)
            .sum();
        g_w = g_w.max(w);
    }
    let g_h = (line_h * lines.len() as u32).max(1);
    let mut img = ImageData { w: g_w, h: g_h, rgba: vec![0u8; (g_w * g_h * 4) as usize] };
    let (cr, cg, cb, ca) = (color[0], color[1], color[2], color[3]);
    for (li, line) in lines.iter().enumerate() {
        let y_base = (li as u32 * line_h) as f32;
        let glyphs: Vec<rusttype::PositionedGlyph> = font
            .layout(line, scale, rusttype::point(0.0, v_metrics.ascent + y_base))
            .collect();
        for g in &glyphs {
            if let Some(bb) = g.pixel_bounding_box() {
                g.draw(|x, y, cov| {
                    let px = bb.min.x as u32 + x;
                    let py = bb.min.y as u32 + y;
                    if px < img.w && py < img.h {
                        let a = cov * ca;
                        let o = ((py * img.w + px) * 4) as usize;
                        img.rgba[o] = (cr * 255.0 * a) as u8;
                        img.rgba[o + 1] = (cg * 255.0 * a) as u8;
                        img.rgba[o + 2] = (cb * 255.0 * a) as u8;
                        img.rgba[o + 3] = (a * 255.0) as u8;
                    }
                });
            }
        }
    }
    img
}

impl App {
    /// Compute widget uniform data (12 f32 each). Returns the 96-float layout.
    fn prepare_widgets(&mut self, width: u32, height: u32) -> [f32; 1280] {
        use crate::config::WidgetType;
        let mut data = [0.0f32; 1280];
        let mut tex_index = 0u32;
        // Reserve slot 3 for the album cover (clocks/images use 0..2).
        self.cover_tex_index = 3;
        let widgets: Vec<crate::config::WidgetConfig> = self.cfg.widgets.iter().take(32).cloned().collect();
        for (slot, w) in widgets.iter().enumerate() {
            let o = slot * 40;
            data[o] = match w.widget_type {
                WidgetType::Ring => 0.0,
                WidgetType::Image => 1.0,
                WidgetType::Clock => 2.0,
                WidgetType::Bars => 3.0,
                WidgetType::Cover => 4.0,
                WidgetType::Analog => 5.0,
                WidgetType::Plugin => 6.0,
            };
            data[o + 1] = w.x;
            data[o + 2] = w.y;
            data[o + 3] = w.size;
            data[o + 4] = w.alpha;
            data[o + 5] = w.rotate.to_radians();
            let (cux, cuy, cuw, cuh) = self.widget_uvs[slot];
            data[o + 7] = cux;
            data[o + 8] = cuy;
            data[o + 9] = cuw;
            data[o + 10] = cuh;
            // ring widget style
            data[o + 12] = match w.shape {
                crate::config::Shape::Ring => 0.0,
                crate::config::Shape::Square => 1.0,
                crate::config::Shape::Diamond => 2.0,
                crate::config::Shape::Hexagon => 3.0,
                crate::config::Shape::Triangle => 4.0,
                crate::config::Shape::Star => 5.0,
                crate::config::Shape::Flower => 6.0,
            };
            data[o + 13] = w.corners.max(2.0);
            data[o + 14] = w.spikiness.clamp(0.0, 1.0);
            data[o + 15] = match w.color_mode {
                crate::config::ColorMode::Hue => 0.0,
                crate::config::ColorMode::Solid => 1.0,
                crate::config::ColorMode::Gradient => 2.0,
            };
            data[o + 16] = w.dash_count.max(0.0);
            data[o + 17] = w.dash_ratio.clamp(0.0, 1.0);
            data[o + 18] = w.ring_width.max(1.0);
            data[o + 19] = w.base_radius.max(0.01);
            data[o + 20] = w.growth.max(0.0);
            data[o + 21] = w.halo_strength.clamp(0.0, 1.0);
            data[o + 22] = w.halo_size.max(0.0);
            data[o + 39] = match w.band_mode {
                crate::config::BandMode::Full => 0.0,
                crate::config::BandMode::Bass => 1.0,
                crate::config::BandMode::Mid => 2.0,
                crate::config::BandMode::Treble => 3.0,
                crate::config::BandMode::Energy => 4.0,
            };
            // palette at 23..39
            let pal = if w.colors.len() >= 4 {
                &w.colors[..4]
            } else if w.colors.len() >= 1 {
                &w.colors[..1]
            } else {
                &[[0.404, 0.314, 0.643, 1.0]]
            };
            for (ci, col) in pal.iter().enumerate() {
                let co = o + 23 + ci * 4;
                data[co] = col[0];
                data[co + 1] = col[1];
                data[co + 2] = col[2];
                data[co + 3] = col[3];
            }
            match w.widget_type {
                WidgetType::Plugin => {
                    // tex index points at the plugin's render slot (8 + plugin index)
                    let pidx = w
                        .plugin
                        .as_ref()
                        .and_then(|n| self.plugins.iter().position(|p| p.name() == n))
                        .unwrap_or(0);
                    let ti = (8 + pidx) as u32;
                    data[o + 6] = ti as f32;
                    data[o + 11] = 1.0; // square aspect default; updated when rendered
                    if tex_index <= ti {
                        tex_index = ti + 1;
                    }
                }
                WidgetType::Ring => {
                    data[o + 6] = 0.0;
                    data[o + 7] = 0.0;
                    data[o + 8] = 0.0;
                }
                WidgetType::Analog => {
                    // 18=tickCount, 19=hour angle, 20=minute angle, 21=second angle,
                    // 22=dial border, colors[0]=hand colour
                    data[o + 18] = w.tick_count.clamp(2.0, 24.0);
                    data[o + 22] = w.dial_border.max(0.0);
                    for (ci, ch) in w.color.iter().enumerate() {
                        data[o + 23 + ci] = *ch;
                    }
                    // hand angles (radians, 12 o'clock = -PI/2)
                    let t = now_hmsparts();
                    let sec = t.2 as f32 + t.3 as f32;
                    let min = t.1 as f32 + sec / 60.0;
                    let hour = (t.0 as f32 % 12.0) + min / 60.0;
                    data[o + 19] = (hour / 12.0 * 6.28318530718 - 1.5707963268);
                    data[o + 20] = (min / 60.0 * 6.28318530718 - 1.5707963268);
                    data[o + 21] = (sec / 60.0 * 6.28318530718 - 1.5707963268);
                }
                WidgetType::Cover => {
                    self.cover_slot = slot;
                    // tex_index points at the cover texture slot (set when loaded).
                    data[o + 6] = self.cover_tex_index as f32;
                    // 18=border width, 19=cover growth
                    data[o + 18] = w.border_width.max(0.0);
                    data[o + 19] = w.cover_growth.max(0.0);
                    data[o + 11] = self.cover_aspect;
                    // border colour from widget.color -> colors[0]
                    for (ci, ch) in w.color.iter().enumerate() {
                        data[o + 23 + ci] = *ch;
                    }
                    // Pull the latest cover from the MPRIS thread.
                    while let Ok(img) = self.cover_rx.try_recv() {
                        self.cover_loaded = true;
                        self.cover_aspect = img.h as f32 / img.w as f32;
                        self.current_cover = Some(img);
                        self.cover_uploaded = false;
                        log::info!("cover: new cover stored ({}x{})", self.cover_aspect, 0);
                    }
                }
                WidgetType::Bars => {
                    // Reuse style slots: 18=bars count, 19=max height, 20=gap, 21=mirror.
                    data[o + 18] = w.bar_count.clamp(2.0, 64.0);
                    data[o + 19] = w.bar_height.max(0.01);
                    data[o + 20] = w.bar_gap.clamp(0.0, 0.9);
                    data[o + 21] = w.bar_mirror as u32 as f32;
                }
                WidgetType::Image => {
                    // The cover owns slot 3 (set after the cover upload loop below). Skip
                    // it so an Image widget never clobbers the album art.
                    if tex_index as usize == self.cover_tex_index {
                        tex_index += 1;
                    }
                    let src = match &w.source {
                        Some(s) => s.clone(),
                        None => continue,
                    };
                    let img = self.get_image(&src).cloned();
                    if let Some(img) = img {
                        let img = fit_slot(img);
                        let (iw, ih) = (img.w as f32, img.h as f32);
                        data[o + 6] = tex_index as f32;
                        data[o + 11] = ih / iw; // aspect
                        self.texture_slots[tex_index as usize] = Some(img);
                        self.texture_slot_dirty[tex_index as usize] = true;
                        tex_index += 1;
                    }
                }
                WidgetType::Clock => {
                    if tex_index as usize == self.cover_tex_index {
                        tex_index += 1;
                    }
                    let txt = chrono_now();
                    let (cached_text, cw, ch, cached_tex) = &self.clock_cache[slot];
                    let (cw, ch) = (*cw, *ch);
                    let mut ti = *cached_tex;
                    if &txt != cached_text || cw == 0 {
                        // 3x supersampling: sharper text when downscaled on screen.
                        let img = fit_slot(rasterize_text(&self.font, &txt, w.font_size * 3.0, w.color));
                        let (iw, ih) = (img.w, img.h);
                        ti = tex_index;
                        self.texture_slots[ti as usize] = Some(img);
                        self.texture_slot_dirty[ti as usize] = true;
                        self.clock_cache[slot] = (txt.clone(), iw, ih, ti);
                        data[o + 11] = ih as f32 / iw as f32;
                        if ti >= tex_index {
                            tex_index = ti + 1;
                        }
                    } else {
                        data[o + 11] = ch as f32 / cw as f32;
                    }
                    data[o + 6] = ti as f32;
                }
            }
        }
        for (si, w) in widgets.iter().enumerate() {
        }
        data
    }

    fn get_image(&mut self, path: &str) -> Option<&ImageData> {
        // Simple LRU cache (max IMAGE_CACHE_MAX entries); expand ~ in path. Without LRU
        // eviction a misconfigured widget that points at a rotating path (e.g. a log of
        // MPRIS covers) would grow this Vec without bound, eventually OOM-ing.
        const IMAGE_CACHE_MAX: usize = 32;
        let expanded = path.replacen('~', &std::env::var("HOME").unwrap_or_default(), 1);
        if let Some(pos) = self.image_cache.iter().position(|(p, _)| *p == expanded) {
            // Cache hit: move-to-end makes this the most-recently-used entry. Older entries
            // bubble to the front and get evicted first when we exceed the cap.
            if pos != self.image_cache.len() - 1 {
                let entry = self.image_cache.remove(pos);
                self.image_cache.push(entry);
            }
            return self.image_cache.last().map(|(_, d)| d.as_ref());
        }
        if let Some(img) = load_png(&expanded) {
            self.image_cache.push((expanded, std::sync::Arc::new(img)));
            // Evict oldest entries (front of Vec) until we're back under the cap.
            while self.image_cache.len() > IMAGE_CACHE_MAX {
                self.image_cache.remove(0);
            }
            return self.image_cache.last().map(|(_, d)| d.as_ref());
        }
        None
    }

    /// Refresh MPRIS music info (throttled by the cover thread cadence: cheap anyway).
    fn poll_music(&mut self) {
        let meta = |key: &str| -> String {
            std::process::Command::new("playerctl")
                .args(["metadata", key])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_default()
        };
        let title = meta("xesam:title");
        let artist = meta("xesam:artist");
        let album = meta("xesam:album");
        let duration_us = meta("mpris:length")
            .parse::<i64>()
            .unwrap_or(0);

        let music_changed = !title.is_empty() && self.music.title != title;
        if !title.is_empty() {
            self.music.title = title.clone();
        }
        if !artist.is_empty() {
            self.music.artist = artist.clone();
        }

        // Lyric fetching only matters when a lyric style is active.
        if self.cfg.style != config::LyricStyle::Off && music_changed {
            self.request_lyrics(lyrics::TrackRequest {
                title,
                artist,
                album,
                duration_ms: duration_us / 1000,
                source: self.cfg.lyric_source.clone(),
                ttml_url: self.cfg.ttml_url.clone(),
            });
        }

        // Drain any finished lyric fetches.
        while let Ok(result) = self.lyric_rx.try_recv() {
            match result {
                Ok(data) => {
                    log::info!("lyrics: {} lines from {}", data.lines.len(), data.source);
                    self.lyrics = Some(data);
                }
                Err(e) => {
                    log::info!("lyrics: {e}");
                    self.lyrics = None;
                }
            }
        }
    }

    /// Send a lyric fetch for `req` unless one is already pending/cached for the same track.
    fn request_lyrics(&mut self, req: lyrics::TrackRequest) {
        let key = req.key();
        if self.lyric_key == key {
            return; // already fetched or in flight
        }
        self.lyric_key = key;
        self.lyrics = None;
        let _ = self.lyric_worker_tx.send(req);
    }

    /// Track the playback position toward the MPRIS value (refreshed ~6x/s).
    /// Per-character lyric animation uses `t - p.start` to drive its fly-in clock, so
    /// any smoothing here makes every glyph appear a few frames after the beat. The
    /// previous `(target - pos_sec) * (dt * 3.0)` chase added ~100–150ms of visible lag
    /// — noticeable on fast syllables. Now we follow the target directly, snapping on
    /// large jumps (seek / track change) so the user never sees a glide.
    fn update_pos(&mut self, dt: f32) {
        let target = self.pos_us.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1_000_000.0;
        if !self.pos_synced {
            if target > 0.0 {
                self.pos_sec = target;
                self.pos_synced = true;
            }
        } else {
            let jump = (target - self.pos_sec).abs();
            if jump > 1.5 {
                // User seeked or track changed — snap so the lyrics don't glide across
                // the whole song. 1.5s threshold avoids snapping on normal playerctl
                // polling jitter (~10–30ms).
                self.pos_sec = target;
            } else {
                // Light smoothing (5% per frame ≈ 6ms at 30fps) absorbs MPRIS polling
                // jitter without delaying the per-character animation perceptibly.
                self.pos_sec += (target - self.pos_sec) * (dt * 1.5).min(1.0);
            }
        }
    }

    /// Index of the lyric line active at `pos_sec`, or None when the song hasn't started.
    fn active_lyric_index(&self) -> Option<usize> {
        let data = self.lyrics.as_ref()?;
        if data.lines.is_empty() {
            return None;
        }
        let t = self.pos_sec;
        let mut idx = None;
        for (i, l) in data.lines.iter().enumerate() {
            if (l.start_ms as f32 / 1000.0) <= t {
                idx = Some(i);
            }
        }
        idx
    }

    /// Everything the renderer needs this frame: whether lyrics are enabled, the playback
    /// time, per-char quads, and whether the SDF atlas gained glyphs that must be uploaded.
    fn compute_lyric_frame(&mut self, width: u32, height: u32) -> LyricFrame {
        // Reset the per-frame EDT budget and drain any glyphs that were deferred in
        // earlier frames. Without this, a CJK-heavy reveal would spike the EDT cost
        // on a single frame and freeze the renderer.
        self.glyph_atlas.begin_frame();
        let empty = LyricFrame {
            enabled: false,
            time: self.pos_sec,
            words: Vec::new(),
            fx: [0.0; 9],
            atlas_dirty: false,
        };
        let style = self.cfg.style;
        if style == config::LyricStyle::Off {
            return empty;
        }
        let t = self.pos_sec;
        // Instrumental fallback: when no lyrics were fetched, sonnet animates a virtual
        // "♪" staff (folia's virtual staff lines) so the scene never sits empty.
        let mut virtual_data: Option<lyrics::LyricData> = None;
        let active_idx = match self.active_lyric_index() {
            Some(i) => i,
            None => {
                if style == config::LyricStyle::Sonnet {
                    virtual_data = Some(virtual_staff(t));
                    0
                } else {
                    return empty;
                }
            }
        };
        let translation: String;
        // Ensure glyphs for a visible window around the playhead + the active translation.
        {
            let data = match &self.lyrics {
                Some(d) => d,
                None => virtual_data.as_ref().expect("virtual staff"),
            };
            translation = data.lines[active_idx].translation.clone();
            let mut lo = active_idx;
            while lo > 0 && (data.lines[lo - 1].start_ms as f32 / 1000.0) >= t - 30.0 {
                lo -= 1;
            }
            let mut hi = active_idx;
            while hi + 1 < data.lines.len() && (data.lines[hi + 1].start_ms as f32 / 1000.0) <= t + 30.0 {
                hi += 1;
            }
            for i in lo..=hi {
                // Sonnet emits glyphs at all four role weights (900/700/500/300); the atlas
                // must rasterise the same char set for each, or those quads get skipped.
                self.glyph_atlas.ensure_text(&data.lines[i].text, 0);
                self.glyph_atlas.ensure_text(&data.lines[i].text, 1);
                self.glyph_atlas.ensure_text(&data.lines[i].text, 2);
                self.glyph_atlas.ensure_text(&data.lines[i].text, 3);
            }
            self.glyph_atlas.ensure_text(&translation, 0);
            self.glyph_atlas.ensure_text(&translation, 1);
            self.glyph_atlas.ensure_text(&translation, 2);
            self.glyph_atlas.ensure_text(&translation, 3);
        }

        let atlas_dirty = self.glyph_atlas.is_dirty();
        let colors = lyricview::LyricColors::default();
        let seed = self.lyric_key.bytes().fold(0x100000001b3, |h, b| (h ^ b as u64).wrapping_mul(0x9E3779B97F4A7C15)) ^ active_idx as u64;
        let ctx = lyricview::StyleCtx {
            width: width as f32,
            height: height as f32,
            time: t,
            atlas: &self.glyph_atlas,
            colors: &colors,
            seed,
            mg_bg: self.cfg.mg_bg,
            mg_fixed: self.cfg.mg_fixed,
            mg_decor: self.cfg.mg_decor,
            audio: self.lyric_audio,
            post: if self.cfg.post_enabled {
                [self.cfg.post_grain, self.cfg.post_contrast, self.cfg.post_lens_distortion, self.cfg.post_lens_dispersion, self.cfg.post_rgb_shift, self.cfg.post_halftone, self.cfg.post_vignette]
            } else {
                [0.0; 7]
            },
            font_weight: self.cfg.font_weight,
        };
        let quads = {
            let data = match &self.lyrics {
                Some(d) => d,
                None => match &virtual_data {
                    Some(v) => v,
                    None => return LyricFrame { enabled: false, time: t, words: Vec::new(), fx: [0.0; 9], atlas_dirty },
                },
            };
            let input = lyricview::StyleInput {
                lines: &data.lines,
                active_idx,
                translation: &translation,
                song_title: &self.music.title,
                song_artist: &self.music.artist,
                song_album: &self.music.album,
            };
            lyricview::build_frame(style, &ctx, &input)
        };
        LyricFrame {
            enabled: true,
            time: t,
            words: quads.quads.iter().map(|q| q.to_array()).collect(),
            fx: quads.fx.to_array(),
            atlas_dirty,
        }
    }

    /// Throttled lyric diagnostics for on-device debugging.
    fn log_lyric_state(&mut self, frame: &LyricFrame, active_idx: Option<usize>) {
        let now = self.start.elapsed().as_secs_f32();
        if now - self.last_lyric_log < 2.0 {
            return;
        }
        self.last_lyric_log = now;
        log::info!(
            "lyric-state style={:?} enabled={} active={:?} t={:.2} words={} atlas_cells={} dirty={}",
            self.cfg.style,
            frame.enabled,
            active_idx,
            frame.time,
            frame.words.len(),
            self.glyph_atlas.glyph_count(),
            frame.atlas_dirty,
        );
    }

    /// Ask each plugin to render its RGBA texture, then store into texture_slots for
    /// `type: "plugin"` widgets (each plugin owns slot = 8 + plugin index).
    ///
    /// Bug fix: previously this allocated a fresh 1MB buffer every frame and cloned the
    /// resulting RGBA into texture_slots — 30MB/sec of allocator pressure that fragmented
    /// the heap and froze the renderer after long sessions. Now `plugin_buf` is allocated
    /// once at startup and each plugin's RGBA is written in-place into texture_slots[8+i].
    fn render_plugin_textures(&mut self) {
        let n = self.plugins.len();
        let (screen_w, screen_h) = self
            .outputs
            .first()
            .map(|o| (o.width, o.height))
            .unwrap_or((1920, 1080));
        for (i, p) in self.plugins.iter().enumerate() {
            let slot = (8 + i) as u32;
            // Zero the staging buffer (plugin may read stale data otherwise).
            for b in self.plugin_buf.iter_mut() {
                *b = 0;
            }
            let mut req = plugin::RenderRequest {
                slot,
                buf_len: self.plugin_buf.len(),
                buf: self.plugin_buf.as_mut_ptr(),
                update: false,
                width: 0,
                height: 0,
                screen_w,
                screen_h,
            };
            p.bind_state(&self.bands, &self.cfg as *const crate::config::Config);
            p.call_render(&mut req);
            if req.update && req.width > 0 && req.height > 0 {
                let w = req.width.min(512);
                let h = req.height.min(512);
                let ti = 8 + i;
                let needed = (w as usize) * (h as usize) * 4;
                // Grow the texture slot's RGBA only when the size changes; otherwise
                // reuse the existing allocation in place (no clone, no per-frame alloc).
                match self.texture_slots.get_mut(ti) {
                    Some(Some(img)) if img.w == w && img.h == h && img.rgba.len() == needed => {
                        // Reuse: copy row by row from staging buf (stride = 512*4, not w*4).
                        for y in 0..h {
                            let src = (y as usize) * 512 * 4;
                            let dst = (y as usize) * (w as usize) * 4;
                            img.rgba[dst..dst + (w as usize) * 4]
                                .copy_from_slice(&self.plugin_buf[src..src + (w as usize) * 4]);
                        }
                        self.texture_slot_dirty[ti] = true;
                    }
                    _ => {
                        let mut rgba = vec![0u8; needed];
                        for y in 0..h {
                            let src = (y as usize) * 512 * 4;
                            let dst = (y as usize) * (w as usize) * 4;
                            rgba[dst..dst + (w as usize) * 4]
                                .copy_from_slice(&self.plugin_buf[src..src + (w as usize) * 4]);
                        }
                        self.texture_slots[ti] = Some(ImageData { w, h, rgba });
                        self.texture_slot_dirty[ti] = true;
                    }
                }
            }
        }
    }

    /// Timed tick: render only the configured screen (or all if render_screen < 0).
    fn tick(&mut self) {
        let _t_tick = std::time::Instant::now();
        // ---- FPS tracking ----
        // Record this frame's wall-clock time into a rolling window. We log a smoothed
        // FPS once per second so the log stays scannable.
        let now = self.start_time.elapsed().as_secs_f32();
        self.fps_window.push(now);
        // Keep the window at the last 120 frames (~4s @ 30fps).
        if self.fps_window.len() > 120 {
            self.fps_window.remove(0);
        }
        self.total_frames += 1;
        // Log FPS once per second (or on the first frame).
        if (now - self.last_fps_log) >= 1.0 || self.last_fps_log < 0.0 {
            self.last_fps_log = now;
            if self.fps_window.len() >= 2 {
                let span = now - self.fps_window[0];
                let fps = if span > 0.0 {
                    (self.fps_window.len() - 1) as f32 / span
                } else {
                    0.0
                };
                let avg_ms = if self.total_frames > 0 {
                    (now * 1000.0) / self.total_frames as f32
                } else {
                    0.0
                };
                eprintln!(
                    "FPS {:.1} (avg {:.1}ms/frame, n={}, total={})",
                    fps,
                    avg_ms,
                    self.fps_window.len(),
                    self.total_frames
                );
            }
        }
        self.pull_audio();
        let target = self.cfg.render_screen;
        if target >= 0 {
            let idx = target as usize;
            if idx < self.outputs.len() && !self.outputs[idx].closed && self.outputs[idx].width > 0 {
                self.draw_one(idx);
            }
        } else {
            for idx in 0..self.outputs.len() {
                if !self.outputs[idx].closed && self.outputs[idx].width > 0 {
                    self.draw_one(idx);
                }
            }
        }
        // All renderers uploaded the latest SDF atlas this frame; reset the dirty flag.
        self.glyph_atlas.clear_dirty();
        // TICK is measured AFTER all work so the number reflects the real frame cost.
        // Previously it was printed before draw_one → always ~0ms and useless.
        if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
            eprintln!("TICK {:.2}ms", _t_tick.elapsed().as_secs_f64()*1000.0);
        }
    }

    fn pull_audio(&mut self) {
        while let Ok(b) = self.audio_rx.try_recv() {
            self.bands = b;
        }
    }

    fn draw_one(&mut self, idx: usize) {
        let (layer, width, height, closed) = {
            let o = &mut self.outputs[idx];
            (o.layer.clone(), o.width, o.height, o.closed)
        };
        if closed || width == 0 || height == 0 {
            return;
        }

        let elapsed = self.start.elapsed().as_secs_f32();
        self.update_pos(self.frame_dt);
        if elapsed - self.last_music_poll > 2.0 {
            self.last_music_poll = elapsed;
            self.poll_music();
        }
        // Lua hooks: let the script transform bands and tweak config each frame.
        // NOTE: transforms operate on a copy; self.bands stays the raw audio data so the
        // transforms never feed back into themselves (which caused cumulative amplification).
        let mut render_bands = self.lua_state.transform_bands(&self.bands);
        self.lua_state.frame(&mut self.cfg, &self.bands, elapsed, &self.music);
        // Rust plugins: per-frame update + band transform chain.
        let (h, m, s, _) = main_now_hmsparts();
        let cfg_ptr = &self.cfg as *const crate::config::Config;
        for p in self.plugins.iter_mut() {
            let mut bridge = plugin::HostBridge {
                cfg: &mut self.cfg,
                bands: &self.bands,
                log_cb: |msg| log::info!("[plugin] {msg}"),
                now_hms: (h, m, s),
            };
            let ctx = bridge.make_ctx();
            p.set_ctx(ctx);
            p.bind_state(&self.bands, cfg_ptr);
            p.call_update(elapsed);
        }
        for p in &self.plugins {
            let out = p.call_transform(&render_bands);
            // Time-smooth the plugin output (strong low-pass) into the render copy.
            // dt-aware exp smoothing (tau=0.048s attack / 0.203s release) — frame-rate
            // independent, matched to the old alpha=0.5/0.15 blend at 30fps so 60fps
            // doesn't halve the time constants. tau = 0.033 / -ln(1 - old_alpha).
            for i in 0..128 {
                let v = out[i];
                let s = self.plugin_smooth_bands[i];
                let tau = if v > s { 0.048 } else { 0.203 };
                let alpha = 1.0 - (-self.frame_dt / tau).exp();
                let sm = s + (v - s) * alpha;
                self.plugin_smooth_bands[i] = sm;
                render_bands[i] = sm;
            }
        }
        self.render_plugin_textures();
        let spawn_scale = spawn_scale_for(&self.cfg, elapsed);
        let spawn_t = (elapsed / (self.cfg.spawn_duration.max(1.0) / 1000.0)).min(1.0);
        let spawn_effect = match self.cfg.spawn_effect {
            crate::config::SpawnEffect::None => 0u32,
            crate::config::SpawnEffect::Expand => 1u32,
            crate::config::SpawnEffect::Zoom => 2u32,
            crate::config::SpawnEffect::Magic => 3u32,
        };
        let spawn_rot = (self.cfg.spawn_rotate * (1.0 - spawn_t)).to_radians();
        let rotate_rad = (self.cfg.rotate + self.cfg.auto_rotate * elapsed).to_radians();
        let amp_avg = render_bands.iter().copied().sum::<f32>() / NBANDS as f32;
        // Time-domain low-pass: the ring band follows the music smoothly, so the particle
        // orbit swells and settles gently instead of twitching in/out.
        // dt-aware exp smoothing (tau=0.1s) — frame-rate independent, so 60fps doesn't
        // drift the time constant the way the old fixed alpha=0.10 (τ≈0.33s @ 30fps) would.
        let tau = 0.1;
        let alpha = 1.0 - (-self.frame_dt / tau).exp();
        self.ring_amp_smooth += (amp_avg - self.ring_amp_smooth) * alpha;
        let particles = compute_particles(&self.cfg, elapsed, width, height, self.ring_amp_smooth);

        // Widgets need &mut self; do it before borrowing the renderer.
        let mut widgets = self.prepare_widgets(width, height);
        // Audio triplet [bass, vocal, power] for music-reactive lyric particles.
        {
            let bass: f32 = render_bands[..16].iter().copied().sum::<f32>() / 16.0;
            let vocal: f32 = render_bands[40..72].iter().copied().sum::<f32>() / 32.0;
            let power: f32 = amp_avg;
            self.lyric_audio = [bass.min(1.0), vocal.min(1.0), power.min(1.0)];
        }
        let lyric_frame = self.compute_lyric_frame(width, height);
        let lyric_active = self.active_lyric_index();
        self.log_lyric_state(&lyric_frame, lyric_active);
        let renderer = &mut self.outputs[idx].renderer;
        if lyric_frame.atlas_dirty {
            // Drain the dirty cell set and upload only those cells (each CELL×CELL = 16KB),
            // not the whole 16MB atlas. This is the difference between 14ms TICK and the
            // 1–13s spikes we saw when a CJK character first appeared.
            let dirty_cells = self.glyph_atlas.take_dirty_cells();
            if !dirty_cells.is_empty() {
                renderer.upload_lyric_sdf(self.glyph_atlas.atlas_bytes(), &dirty_cells);
            }
        }
        renderer.set_lyrics(lyric_frame.enabled, lyric_frame.time, &lyric_frame.words);
        renderer.set_lyrics_fx(lyric_frame.fx);
        if let Some(path) = &self.capture_path {
            if !self.capture_done && elapsed > 6.0 {
                renderer.request_capture(path);
                self.capture_done = true;
            }
        }
        // Cover texture: upload only when a new cover arrived (not every frame).
        if let Some(img) = &self.current_cover {
            if !self.cover_uploaded {
                if let Some((ux, uy, uw, uh)) = renderer.upload_texture(self.cover_tex_index, &img.rgba, img.w, img.h) {
                    log::info!("cover: uploaded slot={} uv=({:.3},{:.3},{:.3},{:.3})", self.cover_slot, ux, uy, uw, uh);
                    self.widget_uvs[self.cover_slot] = (ux, uy, uw, uh);
                    // also write into the local widgets array so this frame sees it
                    let wo = self.cover_slot * 40;
                    widgets[wo + 7] = ux;
                    widgets[wo + 8] = uy;
                    widgets[wo + 9] = uw;
                    widgets[wo + 10] = uh;
                    self.cover_uploaded = true;
                }
            }
        }
        // Upload only texture slots whose content changed since this renderer last saw them.
        // Static plugin image slots (0..7) become dirty once at load and never again;
        // dynamic plugin slots (8+i) become dirty only when the plugin reported req.update.
        // This replaces the old "re-upload every slot every frame" §5 anti-pattern.
        for (ti, img) in self.texture_slots.iter().enumerate() {
            if self.texture_slot_dirty[ti] {
                if let Some(img) = img {
                    if let Some((ux, uy, uw, uh)) = renderer.upload_texture(ti, &img.rgba, img.w, img.h) {
                        // find the widget slot(s) referencing this texture index
                        for s in 0..32 {
                            let wo = s * 40;
                            if (widgets[wo + 6] - ti as f32).abs() < 0.01 {
                                if ti >= 8 {
                                    log::info!("plugin tex {} -> widget slot {} uv=({:.3},{:.3},{:.3},{:.3})", ti, s, ux, uy, uw, uh);
                                }
                                widgets[wo + 7] = ux;
                                widgets[wo + 8] = uy;
                                widgets[wo + 9] = uw;
                                widgets[wo + 10] = uh;
                                self.widget_uvs[s] = (ux, uy, uw, uh);
                            }
                        }
                    }
                }
                self.texture_slot_dirty[ti] = false;
            }
        }
        renderer.set_widgets(&widgets);
        renderer.resize(width, height);
        renderer.set_auto_rotate(rotate_rad);
        // Precompute 64 bar energies from the render bands (bars widgets look these up).
        let mut bar_energy = [0.0f32; 64];
        {
            let n = render_bands.len();
            for bi in 0..64 {
                let lo = bi * n / 64;
                let hi = ((bi + 1) * n / 64).max(lo + 1);
                let mut acc = 0.0f32;
                for i in lo..hi {
                    acc += render_bands[i];
                }
                bar_energy[bi] = acc / (hi - lo) as f32;
            }
        }
        renderer.set_bar_energy(&bar_energy);
        // Overall energy (mid band mean) for the uniform-mode rings.
        let overall = {
            let mut acc = 0.0f32;
            for i in 16..96 {
                acc += render_bands[i];
            }
            acc / 80.0
        };
        renderer.set_overall_energy(overall);
        let pcount = self.cfg.particles.len().min(MAX_PARTICLES) as u32;
        renderer.set_particle_count(pcount);
        // Particle band centre (px): ring base + actual amp-driven growth + halo + offset.
        // Previously this used `growth * 0.5` (i.e. assumed amp=0.5) which made the
        // shader's pre-filter cull every particle once amp_avg drifted away from 0.5.
        let amp_avg = self.ring_amp_smooth;
        let band_r = (self.cfg.base_radius + self.cfg.growth * amp_avg + self.cfg.halo_size * 0.5
            + self.cfg.particles.first().map(|p| p.x).unwrap_or(0.012)) * (width.min(height) as f32);
        renderer.set_particle_band(band_r);
        renderer.set_render_scale(self.cfg.render_scale);
        renderer.render(&render_bands, spawn_scale, spawn_effect, spawn_t, spawn_rot, &particles, elapsed);

        let surface = layer.wl_surface();
        // Damage only the region where the rings/widgets actually live (centre band +
        // widget zones) instead of the full frame — niri only recomposites damaged
        // regions, so a full-screen damage makes the whole desktop re-composite every frame.
        let dw = width as i32;
        let dh = height as i32;
        // rings occupy the central ~46% height; widgets live near edges — be generous but
        // still far smaller than the full frame.
        let rx0 = (dw / 2 - dw * 4 / 10).max(0);
        let rx1 = (dw / 2 + dw * 4 / 10).min(dw);
        let ry0 = (dh / 2 - dh * 4 / 10).max(0);
        let ry1 = (dh / 2 + dh * 4 / 10).min(dh);
        surface.damage_buffer(rx0, ry0, rx1 - rx0, ry1 - ry0);
        // widgets near the edges
        for s in 0..32 {
            let wo = s * 40;
            let wtype = widgets[wo];
            if wtype > 0.5 && widgets[wo + 4] > 0.004 {
                let wx = (widgets[wo + 1] * width as f32) as i32;
                let wy = (widgets[wo + 2] * height as f32) as i32;
                let ws = (widgets[wo + 3] * (width.min(height)) as f32) as i32;
                surface.damage_buffer((wx - ws).max(0), (wy - ws).max(0), ws * 2, ws * 2);
            }
        }
        layer.commit();
    }
}
#[cfg(test)]
mod tests {
    use crate::config::parse_for_test;

    #[test]
    fn parse_widgets_works() {
        let qml = r##"
PulseRing {
    widgets: [
        Widget { type: "clock"; x: 0.5; y: 0.22; fontSize: 56; color: "#EADDFF"; alpha: 0.9 }
    ]
}
"##;
        let cfg = parse_for_test(qml);
        println!("widgets.len = {}", cfg.widgets.len());
        for w in &cfg.widgets {
            println!("widget: {:?} x={} y={} size={} alpha={}", w.widget_type, w.x, w.y, w.size, w.alpha);
        }
    }

    #[test]
    fn parse_style_works() {
        use crate::config::{LyricStyle, parse_for_test, parse_lyric_style};
        assert_eq!(parse_for_test("PulseRing { style: \"off\" }").style, LyricStyle::Off);
        assert_eq!(parse_for_test("PulseRing { style: \"sonnet\" }").style, LyricStyle::Sonnet);
        assert_eq!(parse_for_test("PulseRing { lyricStyle: \"商籁\" }").style, LyricStyle::Sonnet);
        assert_eq!(parse_for_test("PulseRing { }").style, LyricStyle::Off);
        assert_eq!(parse_lyric_style("商籁"), Some(LyricStyle::Sonnet));
        assert_eq!(parse_lyric_style("nope"), None);
    }
}
