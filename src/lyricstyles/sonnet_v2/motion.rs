//! Folia sonnet v2 — `sonnetMotion.ts` (282 lines) compiler-grade 1:1 port.
//!
//! Pure absolute-time motion evaluation keeps direct seeks identical to
//! continuous playback. Mirrors folia exactly; `clamp01` is duplicated here
//! (as in the TS module which declares its own copy) to preserve module-level
//! self-containment. RNG-dependent entry points (`resolve_sonnet_segment_depth`)
//! take a `&mut dyn FnMut() -> f64` so callers inject the generator (Rust has
//! no global mutable RNG like JS `Math.random`).

use crate::lyricstyles::sonnet_v2::types::{
    SonnetAnimationIntensity, SonnetLayoutDirection, SonnetShot, SonnetShotKind, SonnetTheme,
};

// src/components/visualizer/sonnet/sonnetMotion.ts

/// `clamp01` — folia `sonnetMotion.ts:6`. Mirrors the original module-local copy.
pub fn clamp01(value: f64) -> f64 {
    value.min(1.0).max(0.0)
}

/// `cubicCoordinate` — folia `sonnetMotion.ts:8`.
fn cubic_coordinate(point1: f64, point2: f64, time: f64) -> f64 {
    let inverse = 1.0 - time;
    3.0 * inverse * inverse * time * point1
        + 3.0 * inverse * time * time * point2
        + time * time * time
}

/// `resolveCubicBezier` — folia `sonnetMotion.ts:15`. Resolves CSS-style
/// cubic-bezier timing by solving the x curve before sampling y.
pub fn resolve_cubic_bezier(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    value: f64,
) -> f64 {
    let target = clamp01(value);
    if target == 0.0 || target == 1.0 {
        return target;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    let mut parameter = target;
    for _ in 0..12 {
        let x = cubic_coordinate(x1, x2, parameter);
        if x < target {
            low = parameter;
        } else {
            high = parameter;
        }
        parameter = (low + high) / 2.0;
    }
    cubic_coordinate(y1, y2, parameter)
}

/// `easeSonnetInOut` — folia `sonnetMotion.ts:34`.
pub fn ease_sonnet_in_out(value: f64) -> f64 {
    resolve_cubic_bezier(0.65, 0.0, 0.35, 1.0, value)
}

/// `easeSonnetEnter` — folia `sonnetMotion.ts:35`.
pub fn ease_sonnet_enter(value: f64) -> f64 {
    resolve_cubic_bezier(0.22, 1.0, 0.36, 1.0, value)
}

/// `resolveSonnetAnimationScale` — folia `sonnetMotion.ts:37`.
pub fn resolve_sonnet_animation_scale(theme: &SonnetTheme) -> f64 {
    match theme.animation_intensity {
        SonnetAnimationIntensity::Calm => 0.65,
        SonnetAnimationIntensity::Chaotic => 1.35,
        SonnetAnimationIntensity::Normal => 1.0,
    }
}

// 高张力 PV 风格缓动
/// `easeSonnetExpoOut` — folia `sonnetMotion.ts:42`.
pub fn ease_sonnet_expo_out(value: f64) -> f64 {
    if value == 1.0 {
        1.0
    } else {
        1.0 - 2.0f64.powf(-10.0 * value)
    }
}

/// `easeSonnetElasticOut` — folia `sonnetMotion.ts:45`.
pub fn ease_sonnet_elastic_out(value: f64) -> f64 {
    let p = 0.3;
    2.0f64.powf(-10.0 * value) * ((value - p / 4.0) * (2.0 * std::f64::consts::PI) / p).sin() + 1.0
}

/// `resolveShotProgress` — folia `sonnetMotion.ts:51`.
pub fn resolve_shot_progress(shot: &SonnetShot, time: f64) -> f64 {
    clamp01((time - shot.start_time) / (shot.end_time - shot.start_time).max(0.001))
}

/// `resolveSegmentProgress` — folia `sonnetMotion.ts:55`.
/// Uses ExpoOut for the punchy single-glyph entrance.
pub fn resolve_segment_progress(start_time: f64, end_time: f64, time: f64) -> f64 {
    ease_sonnet_expo_out(clamp01((time - start_time) / (end_time - start_time).max(0.08)))
}

/// `SonnetSegmentRole` alias — the four roles. Kept local to mirror folia's
/// `SonnetSegmentRole` import and `inline `'decoration'` / `'support'`
/// comparisons in this module without dragging in the typography-layer enum.
/// This is the same Rust enum exposed in `types.rs` (`SonnetSegmentRole`); we
/// re-import it for clarity.
pub use crate::lyricstyles::sonnet_v2::types::SonnetSegmentRole;

/// `resolveSonnetSegmentDepth` — folia `sonnetMotion.ts:60`. RNG injected.
pub fn resolve_sonnet_segment_depth(
    role: SonnetSegmentRole,
    random: &mut dyn FnMut() -> f64,
) -> f64 {
    if role != SonnetSegmentRole::Decoration {
        return 0.0;
    }
    if random() > 0.5 {
        0.5 + random() * 0.8
    } else {
        -0.5 - random() * 0.8
    }
}

/// `resolveSonnetSegmentNormalOffset` — folia `sonnetMotion.ts:68`.
pub fn resolve_sonnet_segment_normal_offset(
    role: SonnetSegmentRole,
    layout_direction: SonnetLayoutDirection,
    rotation: f64,
    font_size: f64,
    random_value: f64,
) -> (f64, f64) {
    if role != SonnetSegmentRole::Support {
        return (0.0, 0.0);
    }
    let distance = (random_value.min(1.0).max(0.0) * 2.0 - 1.0) * font_size * 0.3;
    let normal_angle = rotation
        + match layout_direction {
            SonnetLayoutDirection::Vertical => 0.0,
            SonnetLayoutDirection::Horizontal => std::f64::consts::PI / 2.0,
        };
    (normal_angle.cos() * distance, normal_angle.sin() * distance)
}

/// `SonnetShotMotionFrame` — folia `sonnetMotion.ts:80`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SonnetShotMotionFrame {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
}

/// `SonnetFocusTimeRange` — folia `sonnetMotion.ts:88`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SonnetFocusTimeRange {
    pub start_time: f64,
    pub end_time: f64,
}

/// `SonnetCameraFocusPoint` — folia `sonnetMotion.ts:93`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SonnetCameraFocusPoint {
    pub x: f64,
    pub y: f64,
}

/// `SONNET_CAMERA_SMOOTHING_SAMPLES` — folia `sonnetMotion.ts:97`.
const SONNET_CAMERA_SMOOTHING_SAMPLES: &[(f64, f64)] = &[
    (-1.0, 1.0),
    (-0.5, 4.0),
    (0.0, 6.0),
    (0.5, 4.0),
    (1.0, 1.0),
];

/// `resolveSonnetSmoothedCameraFocus` — folia `sonnetMotion.ts:105`. Applies
/// deterministic edge-preserving temporal smoothing without tying camera motion
/// to frame rate. `sample_focus` injected as a closure.
pub fn resolve_sonnet_smoothed_camera_focus(
    time: f64,
    start_time: f64,
    end_time: f64,
    sample_focus: &dyn Fn(f64) -> SonnetCameraFocusPoint,
    smoothing_window: f64,
    max_blend_distance: f64,
) -> SonnetCameraFocusPoint {
    let safe_start = start_time.min(end_time);
    let safe_end = start_time.max(end_time);
    let radius = smoothing_window.max(0.0);
    if radius == 0.0 || safe_start == safe_end {
        return sample_focus(time.min(safe_end).max(safe_start));
    }
    let mut samples: [(SonnetCameraFocusPoint, f64); 5] = [(
        SonnetCameraFocusPoint::default(),
        0.0,
    ); 5];
    for (i, &(offset, weight)) in SONNET_CAMERA_SMOOTHING_SAMPLES.iter().enumerate() {
        let sample_time = time.min(safe_end).max(safe_start + 0.0); // unused variable guard below
        let _ = sample_time;
        let sample_time = (time + offset * radius).min(safe_end).max(safe_start);
        samples[i] = (sample_focus(sample_time), weight);
    }
    let center = samples[2].0;
    let max_distance_squared = max_blend_distance.max(0.0).powi(2);
    let mut x = 0.0;
    let mut y = 0.0;
    let mut total_weight = 0.0;
    for &(point, weight) in &samples {
        let distance_squared =
            (point.x - center.x).powi(2) + (point.y - center.y).powi(2);
        // Preserve intentional composition jumps instead of averaging two distant focal points.
        if distance_squared > max_distance_squared {
            continue;
        }
        x += point.x * weight;
        y += point.y * weight;
        total_weight += weight;
    }
    SonnetCameraFocusPoint {
        x: x / total_weight,
        y: y / total_weight,
    }
}

/// `resolveSonnetFocusWeights` — folia `sonnetMotion.ts:134`. Produces stable
/// normalized focus weights, including silent gaps and the tail after the final glyph.
pub fn resolve_sonnet_focus_weights(
    ranges: &[SonnetFocusTimeRange],
    time: f64,
    sigma: f64,
) -> Vec<f64> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let safe_sigma = sigma.max(0.001);
    let log_weights: Vec<f64> = ranges
        .iter()
        .map(|range| {
            let start_time = range.start_time.min(range.end_time);
            let end_time = range.start_time.max(range.end_time);
            let distance = if time < start_time {
                start_time - time
            } else if time > end_time {
                time - end_time
            } else {
                0.0
            };
            -(distance * distance) / (2.0 * safe_sigma * safe_sigma)
        })
        .collect();
    let max_log_weight = log_weights
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = log_weights
        .iter()
        .map(|w| (w - max_log_weight).exp())
        .collect();
    let total_weight: f64 = weights.iter().sum();
    weights.iter().map(|w| w / total_weight).collect()
}

// PV 风格镜头路径：ExpoOut 快速入场、中段近匀速漂移（速度永不为 0）、末段柔和收尾让速给转场。
/// `resolveShotPathProgress` — folia `sonnetMotion.ts:153`.
pub fn resolve_shot_path_progress(kind: SonnetShotKind, progress: f64) -> f64 {
    let linear = clamp01(progress);
    if matches!(
        kind,
        SonnetShotKind::TrackingRibbon
            | SonnetShotKind::FragmentCollage
            | SonnetShotKind::QuietTableau
            | SonnetShotKind::PosterBlocks
    ) {
        // Blend a constant-velocity drift into the inout curve so the middle never stalls.
        return linear * 0.55 + ease_sonnet_in_out(linear) * 0.45;
    }
    if linear < 0.18 {
        return ease_sonnet_expo_out(linear / 0.18) * 0.22;
    }
    if linear < 0.78 {
        return 0.22 + ((linear - 0.18) / 0.6) * 0.56;
    }
    let settle = (linear - 0.78) / 0.22;
    0.78 + (1.0 - (1.0 - settle) * (1.0 - settle)) * 0.22
}

/// `resolveShotMotionFrame` — folia `sonnetMotion.ts:158`. Gives every shot a
/// deliberate, seek-safe camera path instead of relying on audio jitter.
pub fn resolve_shot_motion_frame(kind: SonnetShotKind, progress: f64) -> SonnetShotMotionFrame {
    let linear = clamp01(progress);
    let eased = resolve_shot_path_progress(kind, linear);
    match kind {
        SonnetShotKind::EditorialColumn => SonnetShotMotionFrame {
            x: -0.055 + eased * 0.095,
            y: 0.025 - eased * 0.04,
            scale: 0.98 + eased * 0.07,
            rotation: -0.006 + eased * 0.01,
        },
        SonnetShotKind::TypeImpact => SonnetShotMotionFrame {
            x: -0.035 + eased * 0.07,
            y: 0.018 - eased * 0.028,
            scale: 1.0 + (1.0 - ease_sonnet_expo_out((linear / 0.18).min(1.0))) * 0.22
                + eased * 0.08,
            rotation: -0.01 + eased * 0.016,
        },
        SonnetShotKind::FragmentCollage => SonnetShotMotionFrame {
            x: -0.045 + eased * 0.085,
            y: 0.028 - (eased * std::f64::consts::PI).sin() * 0.055,
            scale: 0.97 + eased * 0.09,
            rotation: -0.014 + eased * 0.028,
        },
        SonnetShotKind::TrackingRibbon => SonnetShotMotionFrame {
            x: -0.16 + eased * 0.28,
            y: 0.05 - eased * 0.085,
            scale: 0.98 + eased * 0.07,
            rotation: 0.008 - eased * 0.014,
        },
        SonnetShotKind::MaskReveal => SonnetShotMotionFrame {
            x: 0.035 - eased * 0.065,
            y: 0.1 - eased * 0.135,
            scale: 0.96 + eased * 0.12,
            rotation: -0.006 + eased * 0.009,
        },
        SonnetShotKind::PosterBlocks => SonnetShotMotionFrame {
            x: -0.012 + eased * 0.024,
            y: 0.008 - eased * 0.016,
            scale: 0.99 + eased * 0.025,
            rotation: -0.0015 + eased * 0.003,
        },
        SonnetShotKind::QuietTableau => SonnetShotMotionFrame {
            x: -0.022 + eased * 0.04,
            y: 0.014 - eased * 0.025,
            scale: 1.0 + eased * 0.028,
            rotation: -0.002 + eased * 0.003,
        },
    }
}

/// `SONNET_CAMERA_BREATH_MAX_OFFSET` — folia `sonnetMotion.ts:240`.
pub const SONNET_CAMERA_BREATH_MAX_OFFSET: f64 = 0.006;
/// `SONNET_CAMERA_BREATH_MAX_SCALE` — folia `sonnetMotion.ts:241`.
pub const SONNET_CAMERA_BREATH_MAX_SCALE: f64 = 0.002;
/// `SONNET_CAMERA_BREATH_MAX_ROTATION` — folia `sonnetMotion.ts:242`.
pub const SONNET_CAMERA_BREATH_MAX_ROTATION: f64 = 0.0015;

/// `resolveSonnetCameraBreath` — folia `sonnetMotion.ts:246`. Deterministic
/// hand-held breathing float: layered incommensurate sines keep the drift
/// organic; absolute-time evaluation keeps direct seeks identical to playback.
pub fn resolve_sonnet_camera_breath(time: f64, phase: f64) -> SonnetShotMotionFrame {
    let tau = time * std::f64::consts::PI * 2.0;
    SonnetShotMotionFrame {
        x: ((tau * 0.13 + phase).sin() * 0.65
            + (tau * 0.31 + phase * 1.7).sin() * 0.35)
            * SONNET_CAMERA_BREATH_MAX_OFFSET,
        y: ((tau * 0.11 + phase * 2.3).cos() * 0.65
            + (tau * 0.29 + phase * 0.9).sin() * 0.35)
            * SONNET_CAMERA_BREATH_MAX_OFFSET,
        scale: (tau * 0.09 + phase * 1.3).sin() * SONNET_CAMERA_BREATH_MAX_SCALE,
        rotation: (tau * 0.07 + phase * 2.9).sin() * SONNET_CAMERA_BREATH_MAX_ROTATION,
    }
}

/// `resolveSonnetBreathWeight` — folia `sonnetMotion.ts:254`. Ramps the
/// breathing float in after the lyric reveal completes so it never pops in
/// mid-line.
pub fn resolve_sonnet_breath_weight(
    time: f64,
    reveal_done_time: f64,
    ramp_duration: f64,
) -> f64 {
    if ramp_duration <= 0.0 {
        return if time >= reveal_done_time { 1.0 } else { 0.0 };
    }
    ease_sonnet_in_out(clamp01((time - reveal_done_time) / ramp_duration))
}

// 纯时间轴伪随机震颤
/// `resolveTimelineShake` — folia `sonnetMotion.ts:260`.
pub fn resolve_timeline_shake(time: f64, intensity: f64) -> SonnetShotMotionFrame {
    if intensity <= 0.0 {
        return SonnetShotMotionFrame {
            x: 0.0,
            y: 0.0,
            scale: 0.0,
            rotation: 0.0,
        };
    }
    // 高频噪点
    let shake_x = (time * 123.456).sin() * (time * 789.123).cos();
    let shake_y = (time * 345.678).cos() * (time * 901.234).sin();
    let shake_rot = (time * 567.890).sin();
    SonnetShotMotionFrame {
        x: shake_x * 0.02 * intensity,
        y: shake_y * 0.02 * intensity,
        scale: 0.0,
        rotation: shake_rot * 0.005 * intensity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(start: f64, end: f64) -> SonnetShot {
        SonnetShot {
            id: String::new(),
            kind: SonnetShotKind::FragmentCollage,
            start_time: start,
            end_time: end,
            line_indices: Vec::new(),
            cues: Vec::new(),
            camera: crate::lyricstyles::sonnet_v2::types::SonnetCameraFrame {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
                rotation: 0.0,
            },
        }
    }

    #[test]
    fn clamp01_clamps_to_unit_interval() {
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.0), 0.0);
        assert_eq!(clamp01(0.5), 0.5);
        assert_eq!(clamp01(1.0), 1.0);
        assert_eq!(clamp01(1.5), 1.0);
    }

    #[test]
    fn resolve_cubic_bezier_endpoints_identity() {
        assert_eq!(resolve_cubic_bezier(0.65, 0.0, 0.35, 1.0, 0.0), 0.0);
        assert_eq!(resolve_cubic_bezier(0.65, 0.0, 0.35, 1.0, 1.0), 1.0);
    }

    #[test]
    fn resolve_cubic_bezier_monotonic_midpoint() {
        // Linear-bezier (y1=0.25, y2=0.75) at 0.5 should sit between endpoints.
        let v = resolve_cubic_bezier(0.0, 0.25, 1.0, 0.75, 0.5);
        assert!(v > 0.0 && v < 1.0, "midpoint {v} not strictly inside");
    }

    #[test]
    fn ease_sonnet_expo_out_endpoints() {
        assert_eq!(ease_sonnet_expo_out(0.0), 0.0);
        assert_eq!(ease_sonnet_expo_out(1.0), 1.0);
    }

    #[test]
    fn resolve_shot_progress_clamps_outside_window() {
        let s = shot(2.0, 4.0);
        assert_eq!(resolve_shot_progress(&s, 1.0), 0.0);
        assert_eq!(resolve_shot_progress(&s, 5.0), 1.0);
    }

    #[test]
    fn resolve_shot_progress_midpoint() {
        let s = shot(2.0, 4.0);
        assert!((resolve_shot_progress(&s, 3.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn resolve_sonnet_segment_depth_zero_for_non_decoration() {
        let mut rng = || 0.99;
        assert_eq!(
            resolve_sonnet_segment_depth(SonnetSegmentRole::Hero, &mut rng),
            0.0
        );
        assert_eq!(
            resolve_sonnet_segment_depth(SonnetSegmentRole::SemiHero, &mut rng),
            0.0
        );
        assert_eq!(
            resolve_sonnet_segment_depth(SonnetSegmentRole::Support, &mut rng),
            0.0
        );
    }

    #[test]
    fn resolve_sonnet_segment_normal_offset_zero_for_non_support() {
        let (x, y) = resolve_sonnet_segment_normal_offset(
            SonnetSegmentRole::Hero,
            SonnetLayoutDirection::Horizontal,
            0.0,
            24.0,
            0.5,
        );
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn resolve_sonnet_segment_normal_offset_support_uses_normal_angle() {
        // rotation = 0, horizontal => normal_angle = pi/2 => cos = 0, sin = 1.
        // random_value = 0.75 => distance = (0.75*2 - 1)*24*0.3 = 3.6
        let (x, y) = resolve_sonnet_segment_normal_offset(
            SonnetSegmentRole::Support,
            SonnetLayoutDirection::Horizontal,
            0.0,
            24.0,
            0.75,
        );
        assert!(x.abs() < 1e-12, "horizontal rotation 0 => x ~ 0, got {x}");
        assert!((y - 3.6).abs() < 1e-9, "y should equal signed distance 3.6, got {y}");
    }

    #[test]
    fn resolve_sonnet_focus_weights_empty() {
        assert!(resolve_sonnet_focus_weights(&[], 0.0, 0.35).is_empty());
    }

    #[test]
    fn resolve_sonnet_focus_weights_single_range_inside_normalizes_to_one() {
        let ranges = [SonnetFocusTimeRange {
            start_time: 0.0,
            end_time: 1.0,
        }];
        let w = resolve_sonnet_focus_weights(&ranges, 0.5, 0.35);
        assert_eq!(w.len(), 1);
        assert!((w[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn resolve_shot_motion_frame_covers_all_variants() {
        // All 7 variants must succeed without panic.
        for kind in [
            SonnetShotKind::EditorialColumn,
            SonnetShotKind::TypeImpact,
            SonnetShotKind::FragmentCollage,
            SonnetShotKind::TrackingRibbon,
            SonnetShotKind::MaskReveal,
            SonnetShotKind::PosterBlocks,
            SonnetShotKind::QuietTableau,
        ] {
            let f = resolve_shot_motion_frame(kind, 0.5);
            // Bounds sanity: x,y within image-plane crop budget, scale near 1.
            assert!(f.x.abs() < 1.0, "{kind:?} x out of bound: {}", f.x);
            assert!(f.y.abs() < 1.0, "{kind:?} y out of bound: {}", f.y);
            assert!(f.scale > 0.8 && f.scale < 1.4, "{kind:?} scale: {}", f.scale);
            assert!(f.rotation.abs() < 0.2, "{kind:?} rotation: {}", f.rotation);
        }
    }

    #[test]
    fn resolve_shot_path_progress_tracking_ribbon_blends() {
        let p = resolve_shot_path_progress(SonnetShotKind::TrackingRibbon, 0.5);
        // 0.5*0.55 + ease_in_out(0.5)*0.45; both halves positive and < 1.
        assert!(p > 0.2 && p < 0.8, "blended value {p} out of range");
    }

    #[test]
    fn resolve_sonnet_camera_breath_small_offset() {
        let f = resolve_sonnet_camera_breath(0.0, 0.0);
        assert!(f.x.abs() <= SONNET_CAMERA_BREATH_MAX_OFFSET);
        assert!(f.y.abs() <= SONNET_CAMERA_BREATH_MAX_OFFSET);
        assert!(f.scale.abs() <= SONNET_CAMERA_BREATH_MAX_SCALE);
        assert!(f.rotation.abs() <= SONNET_CAMERA_BREATH_MAX_ROTATION);
    }

    #[test]
    fn resolve_timeline_shake_zero_intensity_is_zero() {
        let f = resolve_timeline_shake(1.0, 0.0);
        assert_eq!((f.x, f.y, f.rotation), (0.0, 0.0, 0.0));
    }

    #[test]
    fn resolve_timeline_shake_nonzero_intensity_in_budget() {
        let f = resolve_timeline_shake(1.0, 1.0);
        assert!(f.x.abs() <= 0.02, "shake x {x}", x = f.x);
        assert!(f.y.abs() <= 0.02, "shake y {y}", y = f.y);
        assert!(f.rotation.abs() <= 0.005, "shake rot {}", f.rotation);
    }

    #[test]
    fn resolve_sonnet_breath_weight_zero_ramp_is_step_function() {
        assert_eq!(resolve_sonnet_breath_weight(1.0, 2.0, 0.0), 0.0);
        assert_eq!(resolve_sonnet_breath_weight(3.0, 2.0, 0.0), 1.0);
    }
}
