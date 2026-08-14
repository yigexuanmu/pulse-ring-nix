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
// Stage 2: bar lines, D-minor key signature, time-driven playback cursor and beat-pulsing
// noteheads (FFT-coupled via ctx.audio[0]=bass, time-fallback). Instrumental gating is
// already enforced upstream by the ♪ marker in the dispatch loop.
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
    // 3/4 bar lines: 三等分点垂直线
    let accent = ctx.colors.accent;
    for i in 0..2u32 {
        let x = -half_w + (staff_width * (i as f32 + 1.0) / 3.0);
        out.push(StaffElement::Line {
            x0: x, y0: -half_h - 4.0, x1: x, y1: half_h + 4.0,
            thickness: 1.5, alpha: 0.5 * primary[3], color: primary,
        });
    }

    // D-minor key signature: 2 个 flat (♭) markers 在 staff 左侧
    for i in 0..2u32 {
        let y = -half_h + (1.0 + i as f32) * line_spacing;
        let fx = -half_w + 8.0 + i as f32 * 10.0;
        out.push(StaffElement::Ellipse {
            cx: fx, cy: y, rx: 4.0, ry: 3.0,
            alpha: 0.6 * primary[3], color: primary,
        });
        // stem from flat marker down
        out.push(StaffElement::Line {
            x0: fx + 3.0, y0: y - 2.0, x1: fx + 3.0, y1: y + 5.0,
            thickness: 1.0, alpha: 0.5 * primary[3], color: primary,
        });
    }

    // Time-driven playback cursor (sweeps staff_width every 3s)
    let cursor_x = -half_w + (((_time * staff_width / 3.0) % staff_width).max(0.0));
    out.push(StaffElement::Line {
        x0: cursor_x, y0: -half_h - 6.0, x1: cursor_x, y1: half_h + 6.0,
        thickness: 2.0, alpha: 0.65 * accent[3], color: accent,
    });

    // Beat-pulsing noteheads along cursor at 5 staff-line heights; FFT-coupled alpha.
    // ctx.audio[0]=bass dominant; fallback time-based beat if audio silent.
    let bass = ctx.audio[0].max(0.0);
    let beat_pulse = 0.5 + 0.5 * (_time * 2.0).sin(); // ~120 BPM = 2 Hz
    let base_alpha = 0.4 + 0.4 * bass + 0.2 * beat_pulse;
    for i in 0..5u32 {
        let y = -half_h + i as f32 * line_spacing;
        let phase = (_time * 2.0 + i as f32 * 0.4).sin();
        let alpha = (base_alpha + 0.2 * phase).clamp(0.0, 1.0);
        out.push(StaffElement::Ellipse {
            cx: cursor_x, cy: y, rx: 5.0, ry: 4.0,
            alpha: alpha * primary[3], color: primary,
        });
        // stem
        out.push(StaffElement::Line {
            x0: cursor_x + 5.0, y0: y - 2.0, x1: cursor_x + 5.0, y1: y - 14.0,
            thickness: 1.2, alpha: 0.5 * primary[3], color: primary,
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
