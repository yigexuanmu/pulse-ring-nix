//! Per-shot MG scene assembly: HUD background, geometric chaos, fixed geometry, floating
//! particles and the scene-level scanline background. Port of folia `buildSonnetShotMg`
//! (`sonnetShotMg.ts`) plus the `sceneBackgroundLayer` from `sonnetSceneBuilder.ts`.

use crate::lyricview::{CharQuad, SLOT_FRAME};
use crate::lyricstyles::mg::{emit_polygon, MgCanvas, MgXform};
use crate::lyricstyles::mg_geo::Color;

/// Camera transform for the shot (screen-space pan px, zoom and rotation).
#[derive(Debug, Clone, Copy)]
pub struct MgCam {
    pub zoom: f32,
    pub px: f32,
    pub py: f32,
    pub rot: f32,
    pub cx: f32,
    pub cy: f32,
}

#[derive(Debug)]
struct Particle {
    x: f32,
    y: f32,
    size: f32,
    shape: u8, // 0 square, 1 diamond, 2 star
    base_rot: f32,
}

/// The assembled MG layers for one shot.
#[derive(Debug, Default)]
pub struct MgScene {
    scanlines: MgCanvas,
    bg: MgCanvas,
    geo: Option<MgCanvas>,
    geo_rot: f32,
    fixed: Option<MgCanvas>,
    particles: Vec<Particle>,
    width: f32,
    height: f32,
    radius: f32,
    show_bg: bool,
    show_fixed: bool,
    show_decor: bool,
    /// Exponentially smoothed audio energy (folia smoothedIconAudio, attack/release).
    smoothed_audio: f32,
    primary: Color,
    secondary: Color,
    seed: i64,
}

fn rotate(x: f32, y: f32, a: f32) -> [f32; 2] {
    [
        x * a.cos() - y * a.sin(),
        x * a.sin() + y * a.cos(),
    ]
}

/// Shot kinds used by the MG builder (order matches folia's `SonnetShotKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MgShotKind {
    EditorialColumn,
    TypeImpact,
    FragmentCollage,
    TrackingRibbon,
    MaskReveal,
    PosterBlocks,
    QuietTableau,
}

impl MgShotKind {}

/// Build the full MG scene for one shot.
pub fn build_shot_mg(
    kind: MgShotKind,
    width: f32,
    height: f32,
    seed: i64,
    primary: Color,
    secondary: Color,
    accent: Color,
    show_bg: bool,
    show_fixed: bool,
    show_decor: bool,
) -> MgScene {
    // The seed arrives as a large u64 hash cast to i64; bound it to 32 bits so every
    // downstream multiply (seed * 31, seed * 47, ...) stays well inside i64 in debug builds.
    let seed = (seed as u64 & 0xFFFF_FFFF) as i64;
    let radius = width.min(height);
    let mut scanlines = MgCanvas::new();
    {
        // sceneBackgroundLayer: deterministic horizontal tick lines (folia sceneBuilder).
        let scene_seed = seed;
        let density = 9;
        for i in 0..density {
            let x = ((scene_seed + i * 97).rem_euclid(997)) as f32 / 997.0 * width;
            let y = ((scene_seed + i * 193).rem_euclid(991)) as f32 / 991.0 * height;
            let length = 32.0 + ((scene_seed + i * 43).rem_euclid(180)) as f32;
            let color = if i % 2 != 0 { secondary } else { accent };
            let alpha = 0.12 + (i % 4) as f32 * 0.04;
            let w = if i % 3 == 0 { 2.0 } else { 1.0 };
            scanlines.move_to(x - width * 0.5, y - height * 0.5)
                .line_to((x + length).min(width) - width * 0.5, y - height * 0.5)
                .stroke(color, w, alpha);
        }
    }

    let mut bg = MgCanvas::new();
    let mut geo: Option<MgCanvas> = None;
    let mut fixed: Option<MgCanvas> = None;
    let geo_rot;

    // Always-on HUD decoration.
    {
        use crate::lyricstyles::mg_geo::{hud_bg, editorial_bg, scatter_bg, draw_geo_variant, geo_variant_rotation};
        hud_bg(&mut bg, width, height, radius, primary, secondary);
        // Every shot kind gets the geometric-chaos layer + hatch (folia themes a backdrop for
        // each kind; editorial keeps its grid identity instead).
        let variant = std::env::var("PULSE_RING_MG_VARIANT")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_else(|| seed.rem_euclid(48));
        match kind {
            MgShotKind::EditorialColumn | MgShotKind::TypeImpact | MgShotKind::FragmentCollage => {
                editorial_bg(&mut bg, width, height, primary);
                if kind == MgShotKind::EditorialColumn {
                    geo_rot = 0.0;
                } else {
                    let mut g = MgCanvas::new();
                    draw_geo_variant(&mut g, variant, width, height, radius, seed, primary, secondary);
                    geo_rot = geo_variant_rotation(seed, variant);
                    geo = Some(g);

                    let mut fx = MgCanvas::new();
                    fx.rect(-radius * 0.4, -radius * 0.2, radius * 0.6, radius * 0.15).fill(primary, 0.7);
                    fx.rect(-radius * 0.1, radius * 0.1, radius * 0.5, radius * 0.3).stroke(primary, 2.0, 0.6);
                    draw_hatch(&mut fx, -radius * 0.3, -radius * 0.4, radius * 0.4, radius * 0.25, 6.0, primary);
                    fixed = Some(fx);
                }
            }
            _ => {
                scatter_bg(&mut bg, width, height, seed, primary);
                let mut g = MgCanvas::new();
                draw_geo_variant(&mut g, variant, width, height, radius, seed, primary, secondary);
                geo_rot = geo_variant_rotation(seed, variant);
                geo = Some(g);

                let mut fx = MgCanvas::new();
                fx.rect(-radius * 0.4, -radius * 0.2, radius * 0.6, radius * 0.15).fill(primary, 0.7);
                fx.rect(-radius * 0.1, radius * 0.1, radius * 0.5, radius * 0.3).stroke(primary, 2.0, 0.6);
                draw_hatch(&mut fx, -radius * 0.3, -radius * 0.4, radius * 0.4, radius * 0.25, 6.0, primary);
                fixed = Some(fx);
            }
        }
    }

    // Floating particles (no icon textures → geometric shapes only, like folia's fallback).
    let particle_count = if kind == MgShotKind::TypeImpact { 24 } else { 12 };
    let mut particles = Vec::with_capacity(particle_count);
    let hw = width / 2.0;
    let hh = height / 2.0;
    for i in 0..particle_count {
        let i64 = i as i64;
        let p_size = 4.0 + ((seed + i64) % 12) as f32;
        let shape = ((seed + i64).rem_euclid(3)) as u8;
        let x = -hw + width * (((seed * 31 + i64 * 47).rem_euclid(100)) as f32 / 100.0);
        let y = -hh + height * (((seed * 73 + i64 * 19).rem_euclid(100)) as f32 / 100.0);
        let base_rot = ((seed + i64 * 13) % 360) as f32 * std::f32::consts::PI / 180.0;
        particles.push(Particle { x, y, size: p_size, shape, base_rot });
    }

    MgScene {
        scanlines,
        bg,
        geo,
        geo_rot,
        fixed,
        particles,
        width,
        height,
        radius,
        show_bg,
        show_fixed,
        show_decor,
        smoothed_audio: 0.0,
        primary,
        secondary,
        seed,
    }
}

fn draw_hatch(t: &mut MgCanvas, x: f32, y: f32, w: f32, h: f32, spacing: f32, color: Color) {
    let mut i = -w;
    while i < w + h {
        t.move_to(x + i, y).line_to(x + i + h, y + h).stroke(color, 1.0, 0.15);
        i += spacing;
    }
}

impl MgScene {
    /// Emit every layer at the given shot progress (0..1) and playback time. `audio` is
    /// [bass, vocal, power] 0..1 and drives the floating particles (folia updateTime).
    pub fn emit(&mut self, raw_progress: f32, time: f32, shot_start: f32, shot_end: f32, audio: [f32; 3], cam: &MgCam, out: &mut Vec<CharQuad>) {
        let id = MgXform { cx: cam.cx, cy: cam.cy, zoom: 1.0, tx: 0.0, ty: 0.0, rot: 0.0 };
        let full = MgXform { cx: cam.cx, cy: cam.cy, zoom: cam.zoom, tx: cam.px, ty: cam.py, rot: cam.rot };
        let p = raw_progress.clamp(0.0, 1.0);

        if self.show_bg {
            self.scanlines.emit(1.0, &id, out);
            self.bg.emit(p, &full, out);
            if let Some(g) = &mut self.geo {
                let geo_xf = MgXform { rot: cam.rot + self.geo_rot, ..full };
                g.emit(p, &geo_xf, out);
            }
        }
        if self.show_fixed {
            if let Some(fx) = &mut self.fixed {
                let fixed_xf = MgXform { rot: 0.0, ..full };
                fx.emit(p, &fixed_xf, out);
            }
        }
        if self.show_decor {
            let par = MgXform {
                cx: cam.cx,
                cy: cam.cy,
                zoom: cam.zoom,
                tx: cam.px * 0.4,
                ty: cam.py * 0.4,
                rot: cam.rot,
            };
            let layer_rot = (time - shot_start) * 0.05;
            let layer_scale = 1.0 + (cam.zoom - 1.0) * 0.3;
            // Alternate particle tint: squares alternate primary/secondary (folia).
            // Audio energy (folia: bass*0.34 + vocal*0.52 + power*0.14, gated & eased) with
            // exponential smoothing — attack 0.34, release 0.16 (folia smoothedIconAudio).
            let raw_energy = audio[0] * 0.34 + audio[1] * 0.52 + audio[2] * 0.14;
            let gated = ((raw_energy - 0.08) / 0.92).max(0.0);
            let target = (gated.powf(0.68) * 1.35).min(1.0);
            let k = if target > self.smoothed_audio { 0.34 } else { 0.16 };
            self.smoothed_audio += (target - self.smoothed_audio) * k;
            let audio_pulse = self.smoothed_audio;
            let scene_dur = (shot_end - shot_start).max(0.01);
            for (idx, pt) in self.particles.iter().enumerate() {
                // Staggered entry like folia's iconAnimations: phase over (n-1), loop phase
                // from a deterministic hash of the particle seed.
                let n = self.particles.len();
                let entry_phase = if n <= 1 { 0.12 } else { 0.04 + (idx as f32 / (n as f32 - 1.0)) * 0.82 };
                let icon_seed = (self.seed + idx as i64 * 17).abs() as u64;
                let entry_dur = (0.62f32 + (icon_seed % 4) as f32 * 0.08).min(0.08f32.max(scene_dur * 0.18));
                let entry_delay = entry_phase * (scene_dur - entry_dur).max(0.0);
                let entry_prog = ((time - shot_start - entry_delay) / entry_dur.max(0.001)).clamp(0.0, 1.0);
                let entry_eased = 1.0 - (1.0 - entry_prog).powi(3);
                let loop_phase = ((icon_seed % 31) as f32) * 0.2;
                let loop_pulse = ((time - shot_start) * std::f32::consts::PI * 0.7 + loop_phase).sin() * 0.5 + 0.5;
                let alpha = (0.62 * entry_eased * (0.72 + audio_pulse * 0.38 + loop_pulse * 0.03)).min(1.0);
                let scale_k = (0.72 + entry_eased * 0.28) * (1.0 + audio_pulse * 0.42) * (1.0 + loop_pulse * 0.025);
                let [rx, ry] = rotate(pt.x * layer_scale, pt.y * layer_scale, layer_rot);
                let [sx, sy] = par.point(rx, ry);
                let rot = pt.base_rot + layer_rot + cam.rot;
                let s = pt.size * cam.zoom * scale_k;
                match pt.shape {
                    0 => push_square(out, sx, sy, s, rot, alpha, if idx % 2 == 0 { self.primary } else { self.secondary }, cam.rot),
                    1 => push_square(out, sx, sy, s, rot + std::f32::consts::FRAC_PI_4, alpha * 0.83, self.primary, cam.rot),
                    _ => push_star(out, sx, sy, pt.size * 1.5 * scale_k, rot, alpha, self.primary, cam.zoom),
                }
            }
        }
    }
}

fn push_square(out: &mut Vec<CharQuad>, cx: f32, cy: f32, s: f32, rot: f32, alpha: f32, color: Color, _cam_rot: f32) {
    if alpha <= 0.004 || s <= 0.0 {
        return;
    }
    let mut c = color;
    c[3] = alpha;
    out.push(CharQuad {
        glow: SLOT_FRAME,
        uv: [0.0; 4],
        px: [s, s],
        pos: [cx, cy],
        scale: 1.0,
        alpha,
        rotate: rot,
        color: c,
        ext: [0.0; 4],
    });
}

/// 4-point sparkle star (folia's quadratic star shape), filled via ear-clipping.
fn push_star(out: &mut Vec<CharQuad>, cx: f32, cy: f32, s: f32, rot: f32, alpha: f32, color: Color, zoom: f32) {
    if alpha <= 0.004 || s <= 0.0 {
        return;
    }
    let base = [
        [0.0, -s], [s * 0.25, -s * 0.25], [s, 0.0], [s * 0.25, s * 0.25],
        [0.0, s], [-s * 0.25, s * 0.25], [-s, 0.0], [-s * 0.25, -s * 0.25],
    ];
    let mut pts = Vec::with_capacity(8);
    for [x, y] in base {
        let [rx, ry] = rotate(x, y, rot);
        pts.push([cx + rx * zoom, cy + ry * zoom]);
    }
    let mut c = color;
    c[3] = alpha;
    let id = MgXform { cx: 0.0, cy: 0.0, zoom: 1.0, tx: 0.0, ty: 0.0, rot: 0.0 };
    emit_polygon(out, &pts, alpha, c, &id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_geo_variants_build() {
        let primary = [1.0f32, 1.0, 1.0, 1.0];
        let secondary = [0.6f32, 0.66, 0.88, 1.0];
        let accent = [0.85f32, 0.72, 1.0, 1.0];
        for kind in [
            MgShotKind::TypeImpact,
            MgShotKind::FragmentCollage,
            MgShotKind::EditorialColumn,
            MgShotKind::TrackingRibbon,
            MgShotKind::MaskReveal,
            MgShotKind::PosterBlocks,
            MgShotKind::QuietTableau,
        ] {
            for seed in 0..96i64 {
                let mut scene = build_shot_mg(kind, 1920.0, 1080.0, seed * 7919, primary, secondary, accent, true, true, true);
                let cam = MgCam { zoom: 1.0, px: 0.0, py: 0.0, rot: 0.0, cx: 960.0, cy: 540.0 };
                let mut out: Vec<CharQuad> = Vec::new();
                scene.emit(1.0, 10.0, 5.0, 12.0, [0.3, 0.3, 0.3], &cam, &mut out);
                assert!(!out.is_empty(), "kind={kind:?} seed={seed}");
                assert!(out.len() < 20_000, "kind={kind:?} seed={seed} quads={}", out.len());
            }
        }
    }

    #[test]
    fn large_hash_seed_no_overflow() {
        // Real seeds come from a u64 hash cast to i64 — can be huge/negative. Must not overflow.
        let primary = [1.0f32, 1.0, 1.0, 1.0];
        let secondary = [0.6f32, 0.66, 0.88, 1.0];
        let accent = [0.85f32, 0.72, 1.0, 1.0];
        for seed in [
            i64::MAX,
            i64::MIN,
            -9_223_372_036_854_775_000,
            -1i64,
            i64::MAX - 1,
        ] {
            let mut scene = build_shot_mg(MgShotKind::TypeImpact, 1920.0, 1080.0, seed, primary, secondary, accent, true, true, true);
            let cam = MgCam { zoom: 1.0, px: 0.0, py: 0.0, rot: 0.0, cx: 960.0, cy: 540.0 };
            let mut out: Vec<CharQuad> = Vec::new();
            scene.emit(1.0, 10.0, 5.0, 12.0, [0.3, 0.3, 0.3], &cam, &mut out);
            assert!(!out.is_empty());
        }
    }
}
