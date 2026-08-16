//! Folia sonnet v2 — `sonnetTextFixedGeo.ts` (190 lines) compiler-grade 1:1 port.
//!
//! Plans and draws deterministic fixed geometry that sits behind ordinary
//! Sonnet text. The TS creates a `pixi.Graphics` container and chains
//! `moveTo/lineTo/arc/circle/rect/stroke/fill` calls; the Rust port reuses the
//! existing `crate::lyricstyles::mg::MgCanvas` builder, which exposes an
//! identical method surface.
//!
//! Two PIXI-specific concepts are handled explicitly:
//! 1. `graphic.rotation = PI / 4` (rotated-frame variant) becomes an optional
//!    `rotation` field on [`TextFixedGeoBackplate`]; the scene arena applies it
//!    when flattening.
//! 2. `graphic.addChild(hatch)` (orb-hatch variant) becomes an optional nested
//!    `child: Option<MgCanvas>` field — same backplate type recursively owns
//!    the hatch cover just as the PIXI container owned the child Graphics.

use crate::lyricstyles::mg::MgCanvas;
use crate::lyricstyles::sonnet_v2::types::SonnetTheme;

/// folia `sonnetTextFixedGeo.ts` — `SonnetTextFixedGeoPlan`.
///
/// Maps 1:1 to the TS discriminated union. Variant string names match the TS
/// literal-union ordering (drives the `resolve_*` selection math).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetTextFixedGeoHollowVariant {
    StraightFrame,
    RotatedFrame,
    OrbitCrosshair,
    SplitArches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetTextFixedGeoSolidVariant {
    OrbHatch,
    MusicSteps,
    BentLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetTextFixedGeoPlan {
    Hollow(SonnetTextFixedGeoHollowVariant),
    Solid(SonnetTextFixedGeoSolidVariant),
}

/// Ordered list of hollow variants — mirrors `HOLLOW_VARIANTS` array order
/// (used by the round-robin `resolveHollowVariant` selection math).
const HOLLOW_VARIANTS: &[SonnetTextFixedGeoHollowVariant] = &[
    SonnetTextFixedGeoHollowVariant::StraightFrame,
    SonnetTextFixedGeoHollowVariant::RotatedFrame,
    SonnetTextFixedGeoHollowVariant::OrbitCrosshair,
    SonnetTextFixedGeoHollowVariant::SplitArches,
];

/// Ordered list of solid variants — mirrors `solidVariants` array order
/// (used by the `% solidVariants.length` selection math).
const SOLID_VARIANTS: &[SonnetTextFixedGeoSolidVariant] = &[
    SonnetTextFixedGeoSolidVariant::OrbHatch,
    SonnetTextFixedGeoSolidVariant::MusicSteps,
    SonnetTextFixedGeoSolidVariant::BentLines,
];

/// folia `sonnetTextFixedGeo.ts:9` — `resolveHollowVariant`.
///
/// `index = floor(seed / divisor) + offset`; result is
/// `HOLLOW_VARIANTS[((index % L) + L) % L]`. Faithful unsigned modulo.
fn resolve_hollow_variant(seed: f64, divisor: f64, offset: i64) -> SonnetTextFixedGeoHollowVariant {
    let index = (seed / divisor).floor() as i64 + offset;
    let len = HOLLOW_VARIANTS.len() as i64;
    HOLLOW_VARIANTS[((index % len + len) % len) as usize]
}

/// folia `sonnetTextFixedGeo.ts:21` — `resolveSonnetTextFixedGeoPlan`.
pub fn resolve_sonnet_text_fixed_geo_plan(seed: f64, is_chorus_effect: bool) -> SonnetTextFixedGeoPlan {
    if is_chorus_effect {
        let chorus_seed = ((seed % 10.0) + 10.0) % 10.0;
        if chorus_seed < 9.0 {
            return SonnetTextFixedGeoPlan::Hollow(resolve_hollow_variant(
                seed,
                10.0,
                chorus_seed as i64,
            ));
        }
        return SonnetTextFixedGeoPlan::Solid(
            SOLID_VARIANTS[(seed / 10.0).floor() as usize % SOLID_VARIANTS.len()],
        );
    }

    let legacy = ((seed % 4.0) + 4.0) % 4.0;
    if legacy == 1.0 || legacy == 2.0 {
        return SonnetTextFixedGeoPlan::Hollow(resolve_hollow_variant(seed, 4.0, legacy as i64));
    }
    SonnetTextFixedGeoPlan::Solid(SOLID_VARIANTS[(seed / 4.0).floor() as usize % SOLID_VARIANTS.len()])
}

/// folia `sonnetTextFixedGeo.ts:30` — `SonnetTextFixedGeoOptions`.
#[derive(Debug, Clone, Copy)]
pub struct SonnetTextFixedGeoOptions<'a> {
    pub seed: f64,
    pub is_chorus_effect: bool,
    pub font_size: f32,
    pub layout_width: f32,
    pub theme: &'a SonnetTheme,
}

/// The backplate the v2 scene graph owns in arena slot for fixed geo.
///
/// Replaces PIXI's `Graphics` container exactly: a primary canvas + the
/// parent transform fields the TS mutates post-construction.
#[derive(Debug, Default)]
pub struct TextFixedGeoBackplate {
    /// Primary drawing canvas (the `pixi.Graphics` the TS allocates top-level).
    pub canvas: MgCanvas,
    /// `graphic.rotation = Math.PI / 4` (rotated-frame variant).
    pub rotation: f32,
    /// `graphic.addChild(hatch)` (orb-hatch variant).
    pub child: Option<MgCanvas>,
}

/// folia `sonnetTextFixedGeo.ts:39` — `drawMusicSteps`.
fn draw_music_steps(t: &mut MgCanvas, width: f32, height: f32, alpha: f32, theme: &SonnetTheme) {
    let heights = [0.24, 0.35, 0.2, 0.82, 0.3, 0.1, 0.23, 0.16];
    let spacing = width / (heights.len() as f32 + 1.0);
    for (index, &height_ratio) in heights.iter().enumerate() {
        let x = -width / 2.0 + spacing * (index as f32 + 1.0);
        let baseline = height * (0.12 - index as f32 * 0.035);
        let color = if index % 2 == 0 {
            theme.accent_color
        } else {
            theme.secondary_color
        };
        t.move_to(x - spacing * 0.12, baseline - height * height_ratio * 0.5)
            .line_to(x + spacing * 0.12, baseline + height * height_ratio * 0.5)
            .stroke(color, (2.0_f32).max(height * 0.025), alpha * 0.52);
    }
}

/// folia `sonnetTextFixedGeo.ts:54` — `drawBentLines`.
fn draw_bent_lines(t: &mut MgCanvas, width: f32, height: f32, alpha: f32, theme: &SonnetTheme) {
    let line_count = 5;
    for index in 0..line_count {
        let x = -width * 0.34 + index as f32 * width * 0.17;
        let top_y = -height * (0.42 - index as f32 * 0.035);
        let elbow_y = -height * (0.08 - index as f32 * 0.025);
        let bottom_y = height * (0.35 + index as f32 * 0.035);
        let color = if index % 2 == 0 {
            theme.accent_color
        } else {
            theme.secondary_color
        };
        t.move_to(x - width * 0.16, top_y)
            .line_to(x, elbow_y)
            .line_to(x - width * 0.015, bottom_y)
            .stroke(color, (2.0_f32).max(height * 0.022), alpha * 0.52);
    }
}

/// folia `sonnetTextFixedGeo.ts:74` — `drawOrbitCrosshair`.
fn draw_orbit_crosshair(
    t: &mut MgCanvas,
    width: f32,
    height: f32,
    alpha: f32,
    color: [f32; 4],
    secondary_color: [f32; 4],
) {
    let radius = width.min(height) * 0.46;
    t.circle(0.0, 0.0, radius).stroke(color, 1.5, alpha);
    t.circle(-width * 0.17, 0.0, radius * 0.72)
        .stroke(secondary_color, 1.0, alpha * 0.72);
    t.circle(width * 0.17, 0.0, radius * 0.72)
        .stroke(secondary_color, 1.0, alpha * 0.72);
    t.move_to(-width * 0.62, 0.0)
        .line_to(width * 0.62, 0.0)
        .stroke(color, 1.0, alpha * 0.64);
    t.move_to(0.0, -height * 0.62)
        .line_to(0.0, height * 0.62)
        .stroke(color, 1.0, alpha * 0.64);
}

/// folia `sonnetTextFixedGeo.ts:90` — `drawSplitArches`.
///
/// TS `graphic.arc(x, 0, r, Math.PI, 0)` — start=π, end=0, default `false` (ccw).
/// `MgCanvas::arc(cx, cy, r, start, end, ccw)` — passing `false` matches
/// PixiJS's clockwise default for this semicircle.
fn draw_split_arches(
    t: &mut MgCanvas,
    width: f32,
    height: f32,
    alpha: f32,
    color: [f32; 4],
    secondary_color: [f32; 4],
) {
    let half_width = width * 0.46;
    let arch_radius = (width * 0.34).min(height * 0.52);
    for (direction, index) in [(-1.0_f32, 0usize), (1.0_f32, 1usize)].iter().copied() {
        let x = direction * half_width * 0.42;
        let primary = if index == 0 { color } else { secondary_color };
        let secondary = if index == 0 { secondary_color } else { color };
        t.move_to(x - arch_radius * 0.72, height * 0.42)
            .line_to(x - arch_radius * 0.72, 0.0)
            .arc(x, 0.0, arch_radius * 0.72, std::f32::consts::PI, 0.0, false)
            .line_to(x + arch_radius * 0.72, height * 0.42)
            .stroke(primary, 1.5, alpha);
        t.move_to(x - arch_radius * 0.48, height * 0.42)
            .line_to(x - arch_radius * 0.48, 0.0)
            .arc(x, 0.0, arch_radius * 0.48, std::f32::consts::PI, 0.0, false)
            .line_to(x + arch_radius * 0.48, height * 0.42)
            .stroke(secondary, 1.0, alpha * 0.58);
    }
    t.move_to(-half_width, height * 0.42)
        .line_to(half_width, height * 0.42)
        .stroke(color, 2.0, alpha * 0.72);
}

/// folia `sonnetTextFixedGeo.ts:113` — `buildSonnetTextFixedGeo`.
pub fn build_sonnet_text_fixed_geo(options: &SonnetTextFixedGeoOptions<'_>) -> TextFixedGeoBackplate {
    let seed_f = options.seed as f32;
    let theme = options.theme;
    let plan = resolve_sonnet_text_fixed_geo_plan(options.seed, options.is_chorus_effect);

    let mut canvas = MgCanvas::new();

    let color = if seed_f as i64 % 2 == 0 {
        theme.primary_color
    } else {
        theme.secondary_color
    };
    let alpha = (if options.is_chorus_effect { 0.4 } else { 0.25 }) + (seed_f as i64 % 10) as f32 * 0.03;
    let scale_multiplier = if options.is_chorus_effect {
        1.5 + (seed_f as i64 % 5) as f32 * 0.3
    } else {
        1.0
    };
    let width = (options.font_size * 2.5 * scale_multiplier)
        .max(options.layout_width * 0.12 * scale_multiplier);
    let height = (options.font_size * 1.8 * scale_multiplier)
        .max(options.layout_width * 0.08 * scale_multiplier);

    let mut rotation = 0.0;
    let mut child: Option<MgCanvas> = None;

    match plan {
        SonnetTextFixedGeoPlan::Hollow(SonnetTextFixedGeoHollowVariant::OrbitCrosshair) => {
            draw_orbit_crosshair(&mut canvas, width, height, alpha, color, theme.secondary_color);
        }
        SonnetTextFixedGeoPlan::Hollow(SonnetTextFixedGeoHollowVariant::SplitArches) => {
            draw_split_arches(&mut canvas, width, height, alpha, color, theme.secondary_color);
        }
        SonnetTextFixedGeoPlan::Hollow(ref variant) => {
            let (frame_width, frame_height) = match variant {
                SonnetTextFixedGeoHollowVariant::RotatedFrame => (width * 0.8, height * 0.8),
                _ => (width, height),
            };
            canvas
                .rect(-frame_width / 2.0, -frame_height / 2.0, frame_width, frame_height)
                .stroke(color, (1.5_f32).max(options.font_size * 0.02), alpha);
            if options.is_chorus_effect && seed_f as i64 % 2 == 0 {
                canvas
                    .rect(-frame_width * 0.6, -frame_height * 0.6, frame_width * 1.2, frame_height * 1.2)
                    .stroke(color, 1.0, alpha * 0.5);
            }
            if matches!(variant, SonnetTextFixedGeoHollowVariant::RotatedFrame) {
                rotation = std::f32::consts::FRAC_PI_4;
            }
        }
        SonnetTextFixedGeoPlan::Solid(SonnetTextFixedGeoSolidVariant::MusicSteps) => {
            draw_music_steps(&mut canvas, width, height, alpha, theme);
        }
        SonnetTextFixedGeoPlan::Solid(SonnetTextFixedGeoSolidVariant::BentLines) => {
            draw_bent_lines(&mut canvas, width, height, alpha, theme);
        }
        SonnetTextFixedGeoPlan::Solid(SonnetTextFixedGeoSolidVariant::OrbHatch) => {
            let radius = width * 0.5;
            canvas.circle(0.0, 0.0, radius).fill(color, alpha * 0.15);
            let mut hatch = MgCanvas::new();
            let hatch_spacing = (4.0_f32).max(width * 0.05);
            let mut offset = -radius;
            while offset < radius {
                let line_height = (radius * radius - offset * offset).max(0.0).sqrt();
                hatch
                    .move_to(offset + radius * 0.4, -line_height + radius * 0.4)
                    .line_to(offset + radius * 0.4, line_height + radius * 0.4);
                offset += hatch_spacing;
            }
            hatch.stroke(color, 1.5, alpha * 0.6);
            child = Some(hatch);
        }
    }

    TextFixedGeoBackplate {
        canvas,
        rotation,
        child,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::SonnetAnimationIntensity;

    fn theme() -> SonnetTheme {
        SonnetTheme {
            animation_intensity: SonnetAnimationIntensity::Normal,
            primary_color: [0.8, 0.1, 0.2, 1.0],
            secondary_color: [0.1, 0.3, 0.8, 1.0],
            accent_color: [0.9, 0.7, 0.1, 1.0],
        }
    }

    #[test]
    fn resolve_hollow_round_robin_honours_modulo() {
        // seed=10, divisor=10, offset= chorusSeed = ((10%10)+10)%10 = 0.
        // index = floor(10/10) + 0 = 1 → HOLLOW_VARIANTS[1] = RotatedFrame.
        let plan = resolve_sonnet_text_fixed_geo_plan(10.0, true);
        assert_eq!(plan, SonnetTextFixedGeoPlan::Hollow(SonnetTextFixedGeoHollowVariant::RotatedFrame));
    }

    #[test]
    fn resolve_chorus_seed_9_routes_to_solid() {
        // chorus_seed = ((9 % 10) + 10) % 10 = 9 → NOT < 9 → solid branch.
        let plan = resolve_sonnet_text_fixed_geo_plan(9.0, true);
        assert!(
            matches!(plan, SonnetTextFixedGeoPlan::Solid(_)),
            "seed 9 in chorus should hit Solid; got {:?}",
            plan
        );
    }

    #[test]
    fn resolve_non_chorus_legacy1_routes_to_hollow() {
        // legacy = ((1 % 4) + 4) % 4 = 1 → hollow. seed=1 maps hollow via divisor=4.
        // index = floor(1/4)+1 = 0+1 = 1 → HOLLOW_VARIANTS[1] = rotated-frame.
        let plan = resolve_sonnet_text_fixed_geo_plan(1.0, false);
        assert_eq!(plan, SonnetTextFixedGeoPlan::Hollow(SonnetTextFixedGeoHollowVariant::RotatedFrame));
    }

    #[test]
    fn resolve_non_chorus_legacy3_routes_to_solid() {
        // legacy = ((3 % 4) + 4) % 4 = 3 → solid; index = floor(3/4) % 3 = 0 → orb-hatch.
        let plan = resolve_sonnet_text_fixed_geo_plan(3.0, false);
        assert_eq!(plan, SonnetTextFixedGeoPlan::Solid(SonnetTextFixedGeoSolidVariant::OrbHatch));
    }

    #[test]
    fn build_orbit_crosshair_backplate_has_no_rotation_or_child() {
        let theme = theme();
        let options = SonnetTextFixedGeoOptions {
            seed: 10.0,
            is_chorus_effect: false,
            font_size: 32.0,
            layout_width: 1200.0,
            theme: &theme,
        };
        let bp = build_sonnet_text_fixed_geo(&options);
        assert_eq!(bp.rotation, 0.0);
        assert!(bp.child.is_none(), "orbit-crosshair should not have a hatch child");
    }

    #[test]
    fn build_rotated_frame_backplate_carries_pi_over_4_rotation() {
        let theme = theme();
        let options = SonnetTextFixedGeoOptions {
            // legacy = ((seed % 4) + 4) % 4 = 1 with seed=1 → rotated-frame.
            seed: 1.0,
            is_chorus_effect: false,
            font_size: 40.0,
            layout_width: 1000.0,
            theme: &theme,
        };
        let bp = build_sonnet_text_fixed_geo(&options);
        assert!((bp.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert!(bp.child.is_none());
    }

    #[test]
    fn build_orb_hatch_backplate_attaches_hatch_child() {
        let theme = theme();
        // chorus=false, legacy=3 (seed=3) → orb-hatch.
        let options = SonnetTextFixedGeoOptions {
            seed: 3.0,
            is_chorus_effect: false,
            font_size: 40.0,
            layout_width: 1000.0,
            theme: &theme,
        };
        let bp = build_sonnet_text_fixed_geo(&options);
        assert!(
            bp.child.is_some(),
            "orb-hatch should attach hatch as child canvas; rotation should be 0"
        );
        assert_eq!(bp.rotation, 0.0);
    }
}
