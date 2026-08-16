//! Folia sonnet v2 — `sonnetCameraTracking.ts` (45 lines) compiler-grade 1:1 port.
//!
//! Selects render glyphs that may drive the camera and resolves their
//! absolute-time focus. Mirrors folia exactly; the TS `<T extends
//! SonnetCameraTrackingGlyph>` generic is preserved as a Rust trait so
//! concrete glyph types can implement the three required fields without
//! wrapping.

// src/components/visualizer/sonnet/sonnetCameraTracking.ts

/// `SonnetCameraTrackingGlyph` — folia `sonnetCameraTracking.ts:4`. Implemented
/// by any render glyph that may drive the camera. `is_background_shape`
/// defaults to `false` (mirroring the TS `?` optional with `!== true` filter).
pub trait SonnetCameraTrackingGlyph {
    fn base_x(&self) -> f64;
    fn base_y(&self) -> f64;
    fn start_time(&self) -> f64;
    /// `true` when this glyph is a decorative/background shape that must NOT
    /// drive the camera. Default `false` mirrors folia's `!== true` semantics.
    fn is_background_shape(&self) -> bool {
        false
    }
}

/// `SonnetCameraFocusPoint` — return shape of `resolveSonnetSegmentCameraFocus`.
/// Equivalent to the inline TS `{ x: number; y: number }`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SonnetCameraFocusPoint {
    pub x: f64,
    pub y: f64,
}

/// `resolveSonnetCameraTrackingGlyphs` — folia `sonnetCameraTracking.ts:11`.
/// Filters out glyphs flagged as background shapes.
pub fn resolve_sonnet_camera_tracking_glyphs<T: SonnetCameraTrackingGlyph>(
    glyphs: &[T],
) -> Vec<&T> {
    glyphs
        .iter()
        .filter(|g| g.is_background_shape() != true)
        .collect()
}

/// `resolveSonnetSegmentCameraFocus` — folia `sonnetCameraTracking.ts:16`.
/// Interpolates only semantic camera glyphs; decorative render nodes must be
/// filtered first.
pub fn resolve_sonnet_segment_camera_focus<T: SonnetCameraTrackingGlyph>(
    glyphs: &[T],
    time: f64,
    tracking_factor: f64,
) -> SonnetCameraFocusPoint {
    resolve_sonnet_segment_camera_focus_inner(&glyphs, time, tracking_factor)
}

// Default-parameter variant (trackingFactor = 0.5 in TS).
pub fn resolve_sonnet_segment_camera_focus_default<T: SonnetCameraTrackingGlyph>(
    glyphs: &[T],
    time: f64,
) -> SonnetCameraFocusPoint {
    resolve_sonnet_segment_camera_focus(glyphs, time, 0.5)
}

fn resolve_sonnet_segment_camera_focus_inner<T: SonnetCameraTrackingGlyph>(
    glyphs: &[T],
    time: f64,
    tracking_factor: f64,
) -> SonnetCameraFocusPoint {
    if glyphs.is_empty() {
        return SonnetCameraFocusPoint { x: 0.0, y: 0.0 };
    }
    let first = &glyphs[0];
    let last = &glyphs[glyphs.len() - 1];
    let seg_center_x = (first.base_x() + last.base_x()) / 2.0;
    let seg_center_y = (first.base_y() + last.base_y()) / 2.0;
    let apply_factor = |exact_x: f64, exact_y: f64| SonnetCameraFocusPoint {
        x: seg_center_x + (exact_x - seg_center_x) * tracking_factor,
        y: seg_center_y + (exact_y - seg_center_y) * tracking_factor,
    };

    if time <= first.start_time() {
        return apply_factor(first.base_x(), first.base_y());
    }
    if time >= last.start_time() {
        return apply_factor(last.base_x(), last.base_y());
    }

    for index in 0..glyphs.len().saturating_sub(1) {
        let current = &glyphs[index];
        let next = &glyphs[index + 1];
        if time < current.start_time() || time > next.start_time() {
            continue;
        }
        let progress = (time - current.start_time())
            / (next.start_time() - current.start_time()).max(0.001);
        return apply_factor(
            current.base_x() + (next.base_x() - current.base_x()) * progress,
            current.base_y() + (next.base_y() - current.base_y()) * progress,
        );
    }
    apply_factor(first.base_x(), first.base_y())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct Glyph {
        base_x: f64,
        base_y: f64,
        start_time: f64,
        is_background: bool,
    }

    impl SonnetCameraTrackingGlyph for Glyph {
        fn base_x(&self) -> f64 {
            self.base_x
        }
        fn base_y(&self) -> f64 {
            self.base_y
        }
        fn start_time(&self) -> f64 {
            self.start_time
        }
        fn is_background_shape(&self) -> bool {
            self.is_background
        }
    }

    fn g(x: f64, y: f64, t: f64, bg: bool) -> Glyph {
        Glyph { base_x: x, base_y: y, start_time: t, is_background: bg }
    }

    #[test]
    fn filter_drops_background_shapes() {
        let glyphs = [g(0.0, 0.0, 0.0, false), g(1.0, 1.0, 0.5, true), g(2.0, 2.0, 1.0, false)];
        let kept = resolve_sonnet_camera_tracking_glyphs(&glyphs);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].base_x(), 0.0);
        assert_eq!(kept[1].base_x(), 2.0);
    }

    #[test]
    fn focus_empty_returns_origin() {
        let glyphs: [Glyph; 0] = [];
        let p = resolve_sonnet_segment_camera_focus_default(&glyphs, 0.0);
        assert_eq!(p, SonnetCameraFocusPoint::default());
    }

    #[test]
    fn focus_before_first_clamps_to_first() {
        let glyphs = [g(10.0, 20.0, 1.0, false)];
        let p = resolve_sonnet_segment_camera_focus_default(&glyphs, 0.0);
        assert_eq!(p, SonnetCameraFocusPoint { x: 10.0, y: 20.0 });
    }

    #[test]
    fn focus_after_last_clamps_to_last() {
        let glyphs = [g(0.0, 0.0, 0.0, false), g(100.0, 200.0, 1.0, false)];
        let p = resolve_sonnet_segment_camera_focus_default(&glyphs, 5.0);
        // seg_center = (0+100)/2, (0+200)/2 = 50, 100; factor 0.5: x = 50 + (100-50)*0.5 = 75
        assert!((p.x - 75.0).abs() < 1e-9, "x={}", p.x);
        assert!((p.y - 150.0).abs() < 1e-9, "y={}", p.y);
    }

    #[test]
    fn focus_between_two_glyphs_interpolates_linearly() {
        let glyphs = [g(0.0, 0.0, 0.0, false), g(10.0, 20.0, 2.0, false)];
        // time 1.0 is halfway; factor 0.5: seg_center=(5,10), exact=(5,10), x=5+0=5, y=10+0=10
        let p = resolve_sonnet_segment_camera_focus_default(&glyphs, 1.0);
        assert!((p.x - 5.0).abs() < 1e-9, "x={}", p.x);
        assert!((p.y - 10.0).abs() < 1e-9, "y={}", p.y);
    }

    #[test]
    fn tracking_factor_zero_yields_segment_center() {
        let glyphs = [g(0.0, 0.0, 0.0, false), g(10.0, 20.0, 2.0, false)];
        // factor 0 => x = seg_center + 0 = seg_center
        let p = resolve_sonnet_segment_camera_focus(&glyphs, 0.0, 0.0);
        assert!((p.x - 5.0).abs() < 1e-9, "x={}", p.x);
        assert!((p.y - 10.0).abs() < 1e-9, "y={}", p.y);
    }

    #[test]
    fn tracking_factor_one_yields_exact_glyph() {
        let glyphs = [g(0.0, 0.0, 0.0, false), g(10.0, 20.0, 2.0, false)];
        // factor 1 => x = seg_center + (exact - seg_center)*1 = exact
        // At time 0 (clamped to first), exact = first = (0,0)
        let p = resolve_sonnet_segment_camera_focus(&glyphs, 0.0, 1.0);
        assert!((p.x - 0.0).abs() < 1e-9, "x={}", p.x);
        assert!((p.y - 0.0).abs() < 1e-9, "y={}", p.y);
    }
}
