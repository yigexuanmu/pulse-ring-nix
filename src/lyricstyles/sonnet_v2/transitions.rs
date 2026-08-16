//! Folia sonnet v2 — `sonnetTransitions.ts` (152 lines) compiler-grade 1:1 port.
//!
//! Resolves fast, seek-stable monochrome scene transitions without chromatic
//! dispersion. Mirrors folia exactly; borrows `clamp01` / `ease_sonnet_in_out`
//! from the sibling `motion` module (instead of re-declaring private copies).

use crate::lyricstyles::sonnet_v2::motion::{clamp01, ease_sonnet_in_out};
use crate::lyricstyles::sonnet_v2::types::{
    SonnetParagraph, SonnetShot, SonnetTransitionKind, SONNET_TRANSITION_KINDS,
};

// src/components/visualizer/sonnet/sonnetTransitions.ts

/// `SonnetSceneTransitionFrame` — folia `sonnetTransitions.ts:9`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SonnetSceneTransitionFrame {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub alpha: f64,
    pub blur: f64,
    pub glitch: f64,
    pub glitch_seed: f64,
}

/// `IDLE_SONNET_TRANSITION_FRAME` — folia `sonnetTransitions.ts:20`.
pub const IDLE_SONNET_TRANSITION_FRAME: SonnetSceneTransitionFrame = SonnetSceneTransitionFrame {
    x: 0.0,
    y: 0.0,
    scale: 1.0,
    rotation: 0.0,
    alpha: 1.0,
    blur: 0.0,
    glitch: 0.0,
    glitch_seed: 0.0,
};

/// Phase of a transition — `'enter' | 'exit'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetTransitionPhase {
    Enter,
    Exit,
}

/// `resolveBoundaryKind` — folia `sonnetTransitions.ts:35`.
fn resolve_boundary_kind(seed: u32, boundary_index: u32) -> SonnetTransitionKind {
    // `(seed ^ Math.imul(boundaryIndex + 1, 0x9e3779b1)) >>> 0` — JS Math.imul
    // is signed i32 multiply; `>>> 0` converts to unsigned u32. Rust
    // `wrapping_mul` on u32 gives the unsigned result directly.
    let mixed = seed ^ ((boundary_index.wrapping_add(1)).wrapping_mul(0x9e3779b1));
    SONNET_TRANSITION_KINDS[(mixed as usize) % SONNET_TRANSITION_KINDS.len()]
}

/// `resolveSonnetTransitionEffectFrame` — folia `sonnetTransitions.ts:42`.
pub fn resolve_sonnet_transition_effect_frame(
    kind: SonnetTransitionKind,
    phase: SonnetTransitionPhase,
    progress: f64,
    seed: u32,
) -> SonnetSceneTransitionFrame {
    let linear = clamp01(progress);
    let eased = ease_sonnet_in_out(linear);
    let amount = if phase == SonnetTransitionPhase::Exit {
        eased
    } else {
        1.0 - eased
    };

    match kind {
        SonnetTransitionKind::FastBlur => SonnetSceneTransitionFrame {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            alpha: if phase == SonnetTransitionPhase::Exit {
                1.0 - amount
            } else {
                1.0 - amount * 0.82
            },
            blur: amount * 14.0,
            glitch: 0.0,
            glitch_seed: 0.0,
        },
        SonnetTransitionKind::MonoGlitch => {
            let step = (linear * 14.0).floor() as i64;
            SonnetSceneTransitionFrame {
                x: 0.0,
                y: 0.0,
                scale: 1.0,
                rotation: 0.0,
                alpha: if phase == SonnetTransitionPhase::Exit && linear > 0.86 {
                    1.0 - (linear - 0.86) / 0.14
                } else {
                    1.0
                },
                blur: 0.0,
                glitch: amount,
                glitch_seed: (seed as f64) * 0.0001 + (step as f64) * 0.173,
            }
        }
        SonnetTransitionKind::CameraPull => SonnetSceneTransitionFrame {
            x: 0.0,
            y: 0.0,
            // Scene filters use a viewport-sized render surface, so transition scaling exposes its bounds.
            scale: 1.0,
            rotation: 0.0,
            alpha: if phase == SonnetTransitionPhase::Exit {
                1.0 - amount
            } else {
                1.0 - amount * 0.72
            },
            blur: 0.0,
            glitch: 0.0,
            glitch_seed: 0.0,
        },
    }
}

/// `resolveSonnetExitTransitionFrame` — folia `sonnetTransitions.ts:82`.
pub fn resolve_sonnet_exit_transition_frame(
    paragraph: &SonnetParagraph,
    time: f64,
    enabled: bool,
    seed: u32,
) -> SonnetSceneTransitionFrame {
    match paragraph.transition_out.as_ref() {
        Some(transition) if enabled && time >= transition.start_time => {
            let progress =
                (time - transition.start_time) / (transition.end_time - transition.start_time).max(0.001);
            resolve_sonnet_transition_effect_frame(
                transition.kind,
                SonnetTransitionPhase::Exit,
                progress,
                seed,
            )
        }
        _ => IDLE_SONNET_TRANSITION_FRAME,
    }
}

/// `resolveSonnetEnterTransitionFrame` — folia `sonnetTransitions.ts:93`.
pub fn resolve_sonnet_enter_transition_frame(
    kind: Option<SonnetTransitionKind>,
    time_since_start: f64,
    duration: f64,
    enabled: bool,
    seed: u32,
) -> SonnetSceneTransitionFrame {
    if !enabled || kind.is_none() || time_since_start < 0.0 || time_since_start > duration {
        return IDLE_SONNET_TRANSITION_FRAME;
    }
    resolve_sonnet_transition_effect_frame(
        kind.unwrap(),
        SonnetTransitionPhase::Enter,
        time_since_start / duration.max(0.001),
        seed,
    )
}

/// `resolveSonnetShotTransitionFrame` — folia `sonnetTransitions.ts:104`.
/// Gives every layout boundary a short transition; paragraphs commonly contain several shots.
pub fn resolve_sonnet_shot_transition_frame(
    shots: &[SonnetShot],
    active_shot_index: usize,
    time: f64,
    enabled: bool,
    seed: u32,
) -> SonnetSceneTransitionFrame {
    if !enabled || shots.len() < 2 {
        return IDLE_SONNET_TRANSITION_FRAME;
    }
    let current = match shots.get(active_shot_index) {
        Some(s) => s,
        None => return IDLE_SONNET_TRANSITION_FRAME,
    };

    if active_shot_index > 0 {
        let previous = &shots[active_shot_index - 1];
        let duration =
            0.24_f64.min(0.14_f64.max((current.start_time - previous.start_time) * 0.18));
        if time <= current.start_time + duration {
            return resolve_sonnet_enter_transition_frame(
                Some(resolve_boundary_kind(seed, (active_shot_index - 1) as u32)),
                time - current.start_time,
                duration,
                true,
                seed + (active_shot_index as u32) * 97,
            );
        }
    }

    let next = match shots.get(active_shot_index + 1) {
        Some(s) => s,
        None => return IDLE_SONNET_TRANSITION_FRAME,
    };
    let duration = 0.24_f64.min(0.14_f64.max((next.start_time - current.start_time) * 0.18));
    let transition_start = next.start_time - duration;
    if time < transition_start {
        return IDLE_SONNET_TRANSITION_FRAME;
    }
    resolve_sonnet_transition_effect_frame(
        resolve_boundary_kind(seed, active_shot_index as u32),
        SonnetTransitionPhase::Exit,
        (time - transition_start) / duration,
        seed + ((active_shot_index + 1) as u32) * 97,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_frame_is_full_opacity_no_transform() {
        assert_eq!(IDLE_SONNET_TRANSITION_FRAME.alpha, 1.0);
        assert_eq!(IDLE_SONNET_TRANSITION_FRAME.scale, 1.0);
        assert_eq!(IDLE_SONNET_TRANSITION_FRAME.blur, 0.0);
        assert_eq!(IDLE_SONNET_TRANSITION_FRAME.glitch, 0.0);
    }

    #[test]
    fn resolve_boundary_kind_round_robins_through_kinds() {
        // Distinct boundary indices produce distinct kinds modulo cycle length.
        let mut kinds = Vec::new();
        for i in 0..12_u32 {
            let k = resolve_boundary_kind(0x12345678, i);
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        assert!(kinds.len() >= 2, "round-robin should hit >=2 kinds, got {}", kinds.len());
    }

    #[test]
    fn enter_fast_blur_decays_alpha_from_full() {
        let f = resolve_sonnet_transition_effect_frame(
            SonnetTransitionKind::FastBlur,
            SonnetTransitionPhase::Enter,
            0.0,
            1234,
        );
        // Enter at progress 0: eased=0, amount=1 (1-0), so alpha = 1 - 1*0.82 = 0.18.
        assert!((f.alpha - 0.18).abs() < 1e-9, "enter alpha at 0: {}", f.alpha);
        assert_eq!(f.blur, 14.0);
    }

    #[test]
    fn exit_fast_blur_decays_alpha_to_zero() {
        let f = resolve_sonnet_transition_effect_frame(
            SonnetTransitionKind::FastBlur,
            SonnetTransitionPhase::Exit,
            1.0,
            1234,
        );
        assert!((f.alpha - 0.0).abs() < 1e-9, "exit alpha at 1: {}", f.alpha);
        assert!((f.blur - 14.0).abs() < 1e-9);
    }

    #[test]
    fn mono_glitch_exit_at_zero_progress_keeps_full_alpha() {
        let f = resolve_sonnet_transition_effect_frame(
            SonnetTransitionKind::MonoGlitch,
            SonnetTransitionPhase::Exit,
            0.0,
            999,
        );
        // linear=0 not > 0.86, so alpha stays 1.0; glitch eased amount only.
        assert_eq!(f.alpha, 1.0);
        assert_eq!(f.blur, 0.0);
    }

    #[test]
    fn mono_glitch_exit_after_0_86_fades_out() {
        let f = resolve_sonnet_transition_effect_frame(
            SonnetTransitionKind::MonoGlitch,
            SonnetTransitionPhase::Exit,
            1.0,
            1,
        );
        // linear=1 > 0.86: alpha = 1 - (1-0.86)/0.14 = 0
        assert!(f.alpha.abs() < 1e-9, "mono glitch exit at 1: {}", f.alpha);
    }

    #[test]
    fn camera_pull_enter_below_one() {
        let f = resolve_sonnet_transition_effect_frame(
            SonnetTransitionKind::CameraPull,
            SonnetTransitionPhase::Enter,
            0.5,
            42,
        );
        assert!(f.alpha > 0.0 && f.alpha < 1.0, "enter alpha mid: {}", f.alpha);
        assert_eq!(f.blur, 0.0);
        assert_eq!(f.glitch, 0.0);
    }

    #[test]
    fn resolve_sonnet_enter_transition_frame_disabled_returns_idle() {
        let f = resolve_sonnet_enter_transition_frame(Some(SonnetTransitionKind::FastBlur), 0.5, 1.0, false, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_enter_transition_frame_none_kind_returns_idle() {
        let f = resolve_sonnet_enter_transition_frame(None, 0.5, 1.0, true, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_enter_transition_frame_outside_window_returns_idle() {
        // time > duration
        let f = resolve_sonnet_enter_transition_frame(Some(SonnetTransitionKind::FastBlur), 2.0, 1.0, true, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
        // time < 0
        let f = resolve_sonnet_enter_transition_frame(Some(SonnetTransitionKind::FastBlur), -0.5, 1.0, true, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_shot_transition_frame_under_two_shots_idle() {
        let shots = [make_shot(0.0, 1.0)];
        let f = resolve_sonnet_shot_transition_frame(&shots, 0, 0.5, true, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_shot_transition_frame_disabled_idle() {
        let shots = [make_shot(0.0, 1.0), make_shot(1.0, 2.0)];
        let f = resolve_sonnet_shot_transition_frame(&shots, 1, 1.5, false, 0);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_shot_transition_frame_active_inside_enter_window() {
        // 2 shots, gap 1.0 => enter duration = min(0.24, max(0.14, 1.0*0.18)) = 0.18
        let shots = [make_shot(0.0, 1.0), make_shot(1.0, 2.0)];
        // time inside enter window of activeShotIndex=1 (current.start_time + duration = 1.0 + 0.18 = 1.18)
        let f = resolve_sonnet_shot_transition_frame(&shots, 1, 1.05, true, 7);
        assert_ne!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    #[test]
    fn resolve_sonnet_shot_transition_frame_first_shot_no_enter_only_exit() {
        // activeShotIndex = 0 has no previous so enter branch is skipped; only exit before next.
        let shots = [make_shot(0.0, 1.0), make_shot(1.0, 2.0)];
        // Far before transitionStart => idle.
        let f = resolve_sonnet_shot_transition_frame(&shots, 0, 0.5, true, 7);
        assert_eq!(f, IDLE_SONNET_TRANSITION_FRAME);
    }

    fn make_shot(start: f64, end: f64) -> SonnetShot {
        use crate::lyricstyles::sonnet_v2::types::SonnetCameraFrame;
        SonnetShot {
            id: String::new(),
            kind: crate::lyricstyles::sonnet_v2::types::SonnetShotKind::FragmentCollage,
            start_time: start,
            end_time: end,
            line_indices: Vec::new(),
            cues: Vec::new(),
            camera: SonnetCameraFrame { x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 },
        }
    }
}
