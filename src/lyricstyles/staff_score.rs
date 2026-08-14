//! La Folia staff-score vector animation (no-lyrics / instrumental handling).
//!
//! A ♪ codepoint (U+266A) emitted by the virtual-staff fallback must NOT be rasterised
//! into the SDF atlas. Instead it triggers folia's INDEPENDENT five-line staff animation
//! (buildSonnetStaffView): 5 staff lines, written 3/4 bar lines, a D-minor key signature,
//! a time-driven playback cursor sweeping the current measure and noteheads pulsing on
//! the beat. The stroke list is returned in STAFF-LOCAL px (origin = staff centre, +x
//! right, +y down — same convention as the 2D MgCanvas pipeline). The sonnet dispatch loop
//! translates these by the active ♪ placement's (x, y) and pushes them through the
//! existing primitive helpers, so the staff rides the shot camera with the rest of the
//! frame and NO WGSL / SDF change is required.

use crate::lyrics::LyricLine;
use crate::lyricview::{CharQuad, StyleCtx, SLOT_PILL, push_line, push_rect};

/// One stroke of the La Folia staff score, in staff-local px (origin = staff centre).
#[derive(Clone, Copy, Debug)]
pub enum StaffElement {
    /// Axis-aligned line segment, drawn as a rotated thin rect (butt cap).
    Line { x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, alpha: f32, color: [f32; 4] },
    /// Filled axis-aligned rect.
    Rect { x: f32, y: f32, w: f32, h: f32, alpha: f32, color: [f32; 4] },
    /// Filled ellipse (notehead), drawn as a SLOT_PILL pill quad with unequal radii.
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, alpha: f32, color: [f32; 4] },
}

/// Build the La Folia staff-score stroke list for one instrumental (no-lyrics) line.
///
/// `ctx` provides the screen size + theme colours; `line` is the active virtual-staff line
/// (its text is the ♪ marker); `time` is the playback clock in seconds. The returned
/// strokes are centre-relative and the dispatch loop positions them at the ♪ placement.
// TODO fft-coupling: drive note pulse + cursor from FFT band energy (ctx.audio) instead of
// wall-clock time, add bar lines + D-minor key signature + notehead stems, and gate the
// whole staff behind an instrumental-segment-detection hold.
pub fn draw_staff_score(ctx: &StyleCtx, _line: &LyricLine, _time: f32) -> Vec<StaffElement> {
    let primary = ctx.colors.primary;
    let mut out: Vec<StaffElement> = Vec::new();

    // Skeleton: 5 staff lines so the dispatch is visibly wired (proves ♪ no longer falls
    // through to the SDF atlas). Geometry, bar lines, D-minor key signature, the playback
    // cursor and the beat-pulsing noteheads arrive in the next commit.
    let staff_width = ctx.width.max(300.0) * 0.6;
    let line_spacing = 18.0_f32;
    let half_w = staff_width * 0.5;
    let half_h = line_spacing * 2.0;
    for i in 0..5u32 {
        let y = -half_h + i as f32 * line_spacing;
        out.push(StaffElement::Line {
            x0: -half_w, y0: y, x1: half_w, y1: y,
            thickness: 2.0, alpha: 0.3 * primary[3], color: primary,
        });
    }
    out
}

/// Translate a staff-local stroke onto the screen at `(ox, oy)` (the active ♪ placement
/// centre), scale its alpha by `alpha_mul` (the placement's settle / transition alpha), and
/// push it into the sonnet quad stream via the existing 2D primitives.
pub fn emit_staff_element(
    out: &mut Vec<CharQuad>,
    ox: f32,
    oy: f32,
    alpha_mul: f32,
    e: StaffElement,
) {
    match e {
        StaffElement::Line { x0, y0, x1, y1, thickness, alpha, color } => {
            push_line(out, ox + x0, oy + y0, ox + x1, oy + y1, thickness, alpha * alpha_mul, color);
        }
        StaffElement::Rect { x, y, w, h, alpha, color } => {
            push_rect(out, ox + x, oy + y, w, h, alpha * alpha_mul, color);
        }
        StaffElement::Ellipse { cx, cy, rx, ry, alpha, color } => {
            if alpha * alpha_mul <= 0.004 || rx <= 0.0 || ry <= 0.0 {
                return;
            }
            out.push(CharQuad {
                glow: SLOT_PILL,
                uv: [0.0; 4],
                px: [rx * 2.0, ry * 2.0],
                pos: [ox + cx, oy + cy],
                scale: 1.0,
                alpha: alpha * alpha_mul,
                rotate: 0.0,
                color,
                ext: [0.0; 4],
            });
        }
    }
}
