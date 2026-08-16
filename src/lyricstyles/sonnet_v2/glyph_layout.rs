//! Foliation sonnet v2 — `sonnetGlyphLayout.ts` (77 lines) compiler-grade 1:1 port.
//!
//! Maps parser-derived grapheme timing to final glyph coordinates and entrance
//! vectors. Pure-function module consuming the already-ported
//! `SonnetSemanticSegment` (graphemes), `SonnetTypographyPlacement`
//! (per-segment layout rect), `GraphemeTiming`, and `is_sonnet_emphasis_role`.
//! No PIXI dependency.

use crate::lyricstyles::sonnet_v2::typography_layout::SonnetTypographyPlacement;
use crate::lyricstyles::sonnet_v2::typography_roles::is_sonnet_emphasis_role;
use crate::lyricstyles::sonnet_v2::types::{GraphemeTiming, SonnetSemanticSegment};

/// folia `sonnetGlyphLayout.ts` — `SonnetGlyphPlacement`.
#[derive(Clone, Debug)]
pub struct SonnetGlyphPlacement {
    pub char: String,
    pub base_x: f64,
    pub base_y: f64,
    pub enter_x: f64,
    pub enter_y: f64,
    pub entry_rotation: f64,
    pub start_time: f64,
    pub settle_time: f64,
}

/// folia `sonnetGlyphLayout.ts` — `SonnetGlyphMotionWindow`.
#[derive(Clone, Copy, Debug)]
pub struct SonnetGlyphMotionWindow {
    pub start_time: f64,
    pub end_time: f64,
}

/// folia `sonnetGlyphLayout.ts` — `resolveSonnetGlyphMotionDuration(window)`.
///
/// Returns the per-grapheme entrance settle duration in seconds, clamped to
/// the shot window so staggers never overrun the shot.
pub fn resolve_sonnet_glyph_motion_duration(window: SonnetGlyphMotionWindow) -> f64 {
    let shot_duration = (window.end_time - window.start_time).max(0.001);
    let preferred = 1.8_f64.min(0.65_f64.max(shot_duration * 0.42));
    preferred.min(shot_duration * 0.72)
}

/// folia `sonnetGlyphLayout.ts` — `buildSonnetGlyphLayout`.
///
/// `measure_glyph(char) -> f64` mirrors the TS `measureGlyph: (char: string) => number`
/// callback; in the live render shell this is supplied by FreeTypeBackend advance sums.
pub fn build_sonnet_glyph_layout(
    segment: &SonnetSemanticSegment,
    placement: &SonnetTypographyPlacement,
    font_size: f64,
    measure_glyph: &dyn Fn(&str) -> f64,
    motion_window: SonnetGlyphMotionWindow,
) -> Vec<SonnetGlyphPlacement> {
    // TS: `Array.from(segment.text)` — splits on Unicode code points (not grapheme
    // clusters). Rust `chars()` matches exactly for BMP text (which all CJK + latin
    // lyric content is); folia uses the same `Array.from` baseline.
    let fallback_chars: Vec<String> = segment.text.chars().map(|c| c.to_string()).collect();
    let graphemes: Vec<GraphemeTiming> = if !segment.graphemes.is_empty() {
        segment.graphemes.clone()
    } else {
        let len = fallback_chars.len().max(1);
        fallback_chars
            .iter()
            .enumerate()
            .map(|(index, char)| {
                GraphemeTiming {
                    char: char.clone(),
                    start_time: segment.start_time
                        + (segment.end_time - segment.start_time) * (index as f64) / (len as f64),
                    end_time: segment.start_time
                        + (segment.end_time - segment.start_time)
                        * ((index + 1) as f64)
                        / (len as f64),
                    word_index: None,
                }
            })
            .collect()
    };
    let advances: Vec<f64> = graphemes
        .iter()
        .map(|item| {
            if placement.vertical {
                font_size * 0.9
            } else {
                (font_size * 0.2).max(measure_glyph(&item.char))
            }
        })
        .collect();
    let total_advance: f64 = advances.iter().sum();
    let motion_duration = resolve_sonnet_glyph_motion_duration(motion_window);
    let mut cursor = -total_advance / 2.0;

    graphemes
        .iter()
        .enumerate()
        .map(|(index, grapheme)| {
            let advance = advances[index];
            let local_x = if placement.vertical { 0.0 } else { cursor + advance / 2.0 };
            let local_y = if placement.vertical { cursor + advance / 2.0 } else { 0.0 };
            cursor += advance;
            let cosine = placement.rotation.cos();
            let sine = placement.rotation.sin();
            let stagger = if index % 2 == 0 { -1.0 } else { 1.0 };
            let start_time = grapheme.start_time;
            let settle_time = start_time + motion_duration;
            SonnetGlyphPlacement {
                char: grapheme.char.clone(),
                base_x: placement.x + local_x * cosine - local_y * sine,
                base_y: placement.y + local_x * sine + local_y * cosine,
                enter_x: placement.enter_x + if placement.vertical { stagger * font_size * 0.28 } else { 0.0 },
                enter_y: placement.enter_y + if placement.vertical { 0.0 } else { stagger * font_size * 0.24 },
                entry_rotation: stagger * if is_sonnet_emphasis_role(placement.role) { 0.055 } else { 0.035 },
                start_time,
                settle_time: start_time.max(settle_time),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::{
        SonnetCameraFrame, SonnetLayoutDirection, SonnetSegmentRole, SonnetShot, SonnetShotKind,
    };

    fn seg(text: &str, start: f64, end: f64) -> SonnetSemanticSegment {
        SonnetSemanticSegment {
            text: text.into(),
            start_offset: 0,
            end_offset: text.chars().count(),
            start_time: start,
            end_time: end,
            word_indices: vec![],
            graphemes: vec![],
            is_word_like: false,
        }
    }

    fn placement(x: f64, y: f64, vertical: bool, role: SonnetSegmentRole) -> SonnetTypographyPlacement {
        SonnetTypographyPlacement {
            segment_index: 0,
            display_text: String::new(),
            role,
            font_scale: 1.0,
            measured_width: 0.0,
            measured_height: 0.0,
            x,
            y,
            rotation: 0.0,
            enter_x: 0.0,
            enter_y: 0.0,
            vertical,
            layout_direction: SonnetLayoutDirection::Horizontal,
            timing_phase: 0.0,
        }
    }

    #[test]
    fn motion_duration_clamps_to_shot_window() {
        // Very long shot → preferred cap 1.8 wins.
        let w = SonnetGlyphMotionWindow { start_time: 0.0, end_time: 100.0 };
        assert_eq!(resolve_sonnet_glyph_motion_duration(w), 1.8);
        // Short shot → capped to shotDuration * 0.72.
        let w = SonnetGlyphMotionWindow { start_time: 0.0, end_time: 0.5 };
        assert_eq!(resolve_sonnet_glyph_motion_duration(w), 0.36);
        // Tiny shot → min floor 0.65 wins first but 0.72 cap kicks in (0.65 > 0.072).
        let w = SonnetGlyphMotionWindow { start_time: 0.0, end_time: 0.1 };
        assert_eq!(resolve_sonnet_glyph_motion_duration(w), 0.072);
    }

    #[test]
    fn horizontal_layout_lays_graphemes_along_x() {
        let s = seg("abc", 0.0, 3.0);
        let p = placement(0.0, 0.0, false, SonnetSegmentRole::Support);
        // measureGlyph returns a fixed advance; verify cursor advances right.
        let out = build_sonnet_glyph_layout(&s, &p, 16.0, &|_c| 16.0, SonnetGlyphMotionWindow { start_time: 0.0, end_time: 3.0 });
        assert_eq!(out.len(), 3);
        assert!(out[0].base_x < out[1].base_x, "x must increase left-to-right");
        assert!(out[1].base_x < out[2].base_x);
        // vertical=false → enter_y stagger is non-zero (alternating): indices 0/-1, 1/+1.
        assert_ne!(out[0].enter_y, out[1].enter_y);
        assert_eq!(out[0].enter_x, 0.0, "horizontal layout keeps enter_x at baseline");
    }

    #[test]
    fn vertical_layout_lays_graphemes_along_y() {
        let s = seg("abc", 0.0, 3.0);
        let p = placement(0.0, 0.0, true, SonnetSegmentRole::Support);
        let out = build_sonnet_glyph_layout(&s, &p, 16.0, &|_c| 16.0, SonnetGlyphMotionWindow { start_time: 0.0, end_time: 3.0 });
        assert_eq!(out.len(), 3);
        assert!(out[0].base_y < out[1].base_y, "y must increase top-to-bottom in vertical mode");
        // vertical=true → enter_x stagger is non-zero; enter_y stays at baseline.
        assert_ne!(out[0].enter_x, out[1].enter_x);
        assert_eq!(out[0].enter_y, 0.0);
    }

    #[test]
    fn emphasis_role_uses_larger_entry_rotation() {
        let s = seg("ab", 0.0, 2.0);
        let p_support = placement(0.0, 0.0, false, SonnetSegmentRole::Support);
        let p_hero = placement(0.0, 0.0, false, SonnetSegmentRole::Hero);
        let out_s = build_sonnet_glyph_layout(&s, &p_support, 16.0, &|_c| 16.0, SonnetGlyphMotionWindow { start_time: 0.0, end_time: 2.0 });
        let out_h = build_sonnet_glyph_layout(&s, &p_hero, 16.0, &|_c| 16.0, SonnetGlyphMotionWindow { start_time: 0.0, end_time: 2.0 });
        // |entryRotation| for hero (0.055) > support (0.035).
        assert!(out_h[0].entry_rotation.abs() > out_s[0].entry_rotation.abs());
    }

    #[test]
    fn fallback_grapheme_timing_uniform_split_when_empty() {
        let s = seg("你坏", 0.0, 2.0); // empty graphemes Vec → uniform split
        let p = placement(0.0, 0.0, false, SonnetSegmentRole::Support);
        let out = build_sonnet_glyph_layout(&s, &p, 16.0, &|_c| 16.0, SonnetGlyphMotionWindow { start_time: 0.0, end_time: 2.0 });
        // 2 graphemes, duration 2s → first [0.0,1.0), second [1.0,2.0).
        assert_eq!(out.len(), 2);
        assert!((out[0].start_time - 0.0).abs() < 1e-9);
        assert!((out[1].start_time - 1.0).abs() < 1e-9);
        // settle_time = start + motion_duration (clamped by max start).
        // For 2s shot: preferred=min(1.8, max(0.65, 0.84))=0.84; min(0.84, 1.44)=0.84.
        let expected_settle = 1.0 + 0.84;
        assert!((out[1].settle_time - expected_settle).abs() < 1e-9);
    }

    // (suppress unused-import warnings for the diagnostic-only structs)
    #[allow(dead_code)]
    const _: (SonnetShot, SonnetShotKind, SonnetCameraFrame) = (
        SonnetShot {
            id: String::new(),
            kind: SonnetShotKind::FragmentCollage,
            start_time: 0.0,
            end_time: 0.0,
            line_indices: vec![],
            cues: vec![],
            camera: SonnetCameraFrame { x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 },
        },
        SonnetShotKind::FragmentCollage,
        SonnetCameraFrame { x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 },
    );
}
