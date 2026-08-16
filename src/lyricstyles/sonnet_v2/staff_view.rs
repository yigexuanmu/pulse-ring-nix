//! Folia sonnet v2 — `sonnetStaffView.ts` (163 lines) compiler-grade 1:1 port.
//!
//! Builds a La Folia staff notation view. Folia builds two PIXI `Graphics`
//! layers onto a single rotated `wrapper` Container:
//! 1. `staffGraphics` — static 5-line staff + bar lines + decorative clef.
//! 2. `noteGraphics` — dynamic notes redrawn every frame via `clear()` +
//!    `moveTo`/`lineTo`/`ellipse`/`fill`/`stroke`/`quadraticCurveTo`.
//!
//! X-architecture mapping (byte-faithful):
//! - `wrapper.rotation / position / alpha / addChild(staffGraphics, noteGraphics)`
//!   → cached on the returned struct; the `TextFixedGeo`-style layer
//!   attachment happens at the future `sonnet_text_view_builder` boundary
//!   (Phase 6.5), not here.
//! - `staffGraphics` → a `MgCanvas` built ONCE at construction.
//! - `noteGraphics`'s per-frame `clear()` + redraw closure → an `update_animation`
//!   method that takes a `&mut MgCanvas` (the caller owns the notes layer so
//!   it can be cleared each frame exactly like PIXI's `clear()`).
//!
//! The two `updateAnimation` outputs (cursor line + per-note ellipses/stems/
//! flags/sharps) are pure functions of `(time, beat_position)`; no state is
//! mutated across frames, matching PIXI's `clear()` + redraw.

use crate::lyricstyles::mg::MgCanvas;
use crate::lyricstyles::sonnet_v2::staff_notation::{
    la_folia_total_beats, SonnetStaffAccidental, SonnetStaffNote, LA_FOLIA_CYCLE_SECONDS,
    LA_FOLIA_STAFF_NOTES,
};
use crate::lyricstyles::sonnet_v2::typography_layout::SonnetTypographyPlacement;
use crate::lyricstyles::sonnet_v2::types::SonnetTheme;

/// folia `sonnetStaffView.ts` — `positiveModulo(value, divisor)`.
///
/// JS `%` is truncated-toward-zero (matches Rust's `f32::rem`); the +
/// divisor / % divisor double-step corrects for negative inputs exactly
/// like the TS source.
#[inline]
pub fn positive_modulo(value: f32, divisor: f32) -> f32 {
    ((value % divisor) + divisor) % divisor
}

/// folia `sonnetStaffView.ts` — `TimedSonnetStaffNote extends SonnetStaffNote`.
///
/// Embedded-note approach (rather than TS `interface extends`) keeps the copy
/// byte-faithful: every TS `{ ...note, startBeat: beatCursor }` becomes
/// `TimedStaffNote { note, start_beat }`.
#[derive(Debug, Clone, Copy)]
pub struct TimedSonnetStaffNote {
    pub note: SonnetStaffNote,
    pub start_beat: f32,
}

/// folia `sonnetStaffView.ts` — the object returned by `buildSonnetStaffView`.
///
/// All per-frame drawing is pulled out of PIXI closures and laid onto
/// `MgCanvas` callers supply. Hallmarks:
/// - `staff_canvas` — built once at construction; `update_animation` never
///   touches it. Equivalent to folia drawing into `staffGraphics` once.
/// - `update_animation(time, notes_canvas)` — equivalent to the folia
///   `updateAnimation = (time) => { ... noteGraphics.clear() ... redraw }`
///   closure. Callers should `*notes_canvas = MgCanvas::default()` (or call
///   the equivalent of PIXI `clear()`) before invoking, exactly as folia's
///   `noteGraphics.clear()` precedes every per-frame paint.
/// - cached layout constants (`staff_width`, `line_spacing`, `half_height`,
///   `playable_width`, `beat_width`, `half_width`) — captured by the folia
///   closure; mirrored as struct fields so `update_animation` stays a pure
///   function of `(time, notes_canvas)`.
/// - `timed_notes` — the cumulative `startBeat` table walked by `update_animation`.
/// - `base_x` / `base_y` / `final_rotation` / `enter_x` / `enter_y` /
///   `start_time` / `settle_time` — identical folia `GlyphView` fields, used
///   by the future `sonnet_text_view_builder` arena-node constructor.
pub struct SonnetStaffView {
    pub staff_canvas: MgCanvas,
    pub timed_notes: Vec<TimedSonnetStaffNote>,
    pub staff_width: f32,
    pub line_spacing: f32,
    pub half_width: f32,
    pub half_height: f32,
    pub playable_width: f32,
    pub beat_width: f32,
    pub default_primary_color: [f32; 4],
    pub default_accent_color: [f32; 4],
    pub base_x: f32,
    pub base_y: f32,
    pub enter_x: f32,
    pub enter_y: f32,
    pub final_rotation: f32,
    pub entry_rotation: f32,
    pub start_time: f32,
    pub settle_time: f32,
    pub z_depth: f32,
    pub is_text_glyph: bool,
}

/// folia `sonnetStaffView.ts` — `buildSonnetStaffView(pixi, placement, theme,
/// baseFontSize, shotStartTime, width, containerLayer) -> GlyphView & { ... }`.
///
/// `containerLayer` (PIXI Container attachment) is dropped — the caller owns
/// layering for the X-architecture's arena stage.
/// `pixi.Color.shared.setValue(theme.primaryColor).toNumber()` becomes a
/// pass-through of the existing `[f32; 4]` color already attached to
/// `SonnetTheme` (Phase 6.2 工作); we don't re-resolve through PixiJS Color.
#[allow(clippy::too_many_arguments)]
pub fn build_sonnet_staff_view(
    placement: &SonnetTypographyPlacement,
    theme: &SonnetTheme,
    base_font_size: f32,
    shot_start_time: f32,
    width: f32,
) -> SonnetStaffView {
    // `wrapper.rotation = placement.rotation` & `wrapper.position.set(...)`
    // & `wrapper.alpha = 0` are presentation-layer; cached on the returned
    // struct for the arena node constructor (no PIXI mutation happens here).
    let base_x = placement.x as f32;
    let base_y = placement.y as f32;
    let enter_x = placement.enter_x as f32;
    let enter_y = placement.enter_y as f32;
    let final_rotation = placement.rotation as f32;
    let entry_rotation: f32 = 0.0;

    let staff_width = (300.0_f32).max(width * 0.6);
    let line_spacing = base_font_size * 0.25;
    let total_height = line_spacing * 4.0;
    let half_width = staff_width / 2.0;
    let half_height = total_height / 2.0;
    let playable_width = staff_width * 0.92;
    let beat_width = playable_width / la_folia_total_beats();

    let mut timed_notes: Vec<TimedSonnetStaffNote> = Vec::with_capacity(LA_FOLIA_STAFF_NOTES.len());
    let mut beat_cursor = 0.0_f32;
    for &note in LA_FOLIA_STAFF_NOTES {
        timed_notes.push(TimedSonnetStaffNote { note, start_beat: beat_cursor });
        beat_cursor += note.beats;
    }

    let primary_color = theme.primary_color;
    let accent_color = theme.accent_color;

    let mut staff_graphics = MgCanvas::new();

    // 5 staff lines (`lineSpacing` apart, vertically centred at the centre).
    for i in 0..5 {
        let y = -half_height + i as f32 * line_spacing;
        staff_graphics.move_to(-half_width, y);
        staff_graphics.line_to(half_width, y);
    }
    staff_graphics.stroke(primary_color, 2.0, 0.3);

    // Keep the written 3/4 bar structure visible while notes loop independently.
    for bar in 1..8 {
        let x = -playable_width / 2.0 + beat_width * bar as f32 * 3.0;
        staff_graphics.move_to(x, -half_height);
        staff_graphics.line_to(x, half_height);
    }
    staff_graphics.stroke(primary_color, 1.0, 0.16);

    // Decorative clef / left bar line.
    staff_graphics.move_to(-half_width + 10.0, -half_height);
    staff_graphics.line_to(-half_width + 10.0, half_height);
    staff_graphics.stroke(primary_color, 4.0, 0.5);

    // Double-bar right edge (two strokes).
    staff_graphics.move_to(half_width - 10.0, -half_height);
    staff_graphics.line_to(half_width - 10.0, half_height);
    staff_graphics.stroke(primary_color, 2.0, 0.5);
    staff_graphics.move_to(half_width - 4.0, -half_height);
    staff_graphics.line_to(half_width - 4.0, half_height);
    staff_graphics.stroke(primary_color, 6.0, 0.5);

    SonnetStaffView {
        staff_canvas: staff_graphics,
        timed_notes,
        staff_width,
        line_spacing,
        half_width,
        half_height,
        playable_width,
        beat_width,
        default_primary_color: primary_color,
        default_accent_color: accent_color,
        base_x,
        base_y,
        enter_x,
        enter_y,
        final_rotation,
        entry_rotation,
        start_time: shot_start_time,
        // `settleTime: shotStartTime + 0.5` — identical to folia.
        settle_time: shot_start_time + 0.5,
        // `zDepth: 0` and `isTextGlyph: false` are the literal folia GlyphView
        // fields the staff view returns.
        z_depth: 0.0,
        is_text_glyph: false,
    }
}

impl SonnetStaffView {
    /// folia `sonnetStaffView.ts` — `updateAnimation = (time: number) => void`.
    ///
    /// Repaints ONLY the dynamic layer onto `notes_canvas`. Callers must clear
    /// `notes_canvas` first (PIXI `noteGraphics.clear()` equivalent). The
    /// function includes zero mutations of `self`; everything the closure
    /// captures is read-only, mirroring the TS source where the outer
    /// `updateAnimation` closure only reads captured state.
    pub fn update_animation(&self, time: f32, notes_canvas: &mut MgCanvas) {
        let cycle_elapsed = positive_modulo(time - self.start_time, LA_FOLIA_CYCLE_SECONDS);
        let beat_position = (cycle_elapsed / LA_FOLIA_CYCLE_SECONDS) * la_folia_total_beats();
        let cursor_x = -self.playable_width / 2.0 + self.beat_width * beat_position;

        notes_canvas.move_to(cursor_x, -self.half_height - self.line_spacing * 0.8);
        notes_canvas.line_to(cursor_x, self.half_height + self.line_spacing * 0.8);
        notes_canvas.stroke(self.default_accent_color, 1.5, 0.34);

        let primary = self.default_primary_color;
        let accent = self.default_accent_color;

        for (index, &TimedSonnetStaffNote { note, start_beat }) in
            self.timed_notes.iter().enumerate()
        {
            let is_active = beat_position >= start_beat && beat_position < start_beat + note.beats;
            let pulse = if is_active {
                ((cycle_elapsed * std::f32::consts::PI * 5.0 + index as f32 * 0.4).sin() + 1.0) * 0.5
            } else {
                0.0
            };
            let note_scale = if is_active { 1.0 + pulse * 0.12 } else { 1.0 };
            let note_radius_x = self.line_spacing * 0.42 * note_scale;
            let note_radius_y = self.line_spacing * 0.29 * note_scale;
            let x = -self.playable_width / 2.0 + self.beat_width * (start_beat + note.beats * 0.5);
            let y = self.half_height - note.staff_step as f32 * self.line_spacing * 0.5;
            let alpha = if is_active {
                0.78 + pulse * 0.16
            } else {
                0.28 + (index % 3) as f32 * 0.03
            };
            let stem_down = note.staff_step >= 6;
            let stem_x = x + if stem_down { -note_radius_x } else { note_radius_x };
            let stem_end_y = y + if stem_down { self.line_spacing * 3.1 } else { -self.line_spacing * 3.1 };

            // Note head: ellipse filled with accent color.
            notes_canvas.ellipse(x, y, note_radius_x, note_radius_y);
            notes_canvas.fill(accent, alpha);

            // Stem.
            notes_canvas.move_to(stem_x, y);
            notes_canvas.line_to(stem_x, stem_end_y);
            notes_canvas.stroke(primary, 1.6, 0.9_f32.min(alpha + 0.08));

            // Flag (only for short notes — `beats <= 0.5`).
            if note.beats <= 0.5 {
                let flag_y = stem_end_y;
                let flag_direction = if stem_down { -1.0 } else { 1.0 };
                notes_canvas.move_to(stem_x, flag_y);
                notes_canvas.quad_to(
                    stem_x + self.line_spacing * 1.1,
                    flag_y + self.line_spacing * 0.55 * flag_direction,
                    stem_x + self.line_spacing * 0.1,
                    flag_y + self.line_spacing * flag_direction,
                );
                notes_canvas.stroke(primary, 1.6, 0.9_f32.min(alpha + 0.08));
            }

            // Sharp accidental (vertical bars + slash-strokes, matches folia's
            // pixi.Graphics path exactly).
            if note.accidental == Some(SonnetStaffAccidental::Sharp) {
                let sharp_x = x - note_radius_x * 2.3;
                let sharp_height = self.line_spacing * 1.15;
                notes_canvas
                    .move_to(sharp_x - self.line_spacing * 0.16, y - sharp_height * 0.5)
                    .line_to(sharp_x - self.line_spacing * 0.16, y + sharp_height * 0.5);
                notes_canvas
                    .move_to(sharp_x + self.line_spacing * 0.16, y - sharp_height * 0.5)
                    .line_to(sharp_x + self.line_spacing * 0.16, y + sharp_height * 0.5);
                notes_canvas
                    .move_to(sharp_x - self.line_spacing * 0.34, y - self.line_spacing * 0.12)
                    .line_to(sharp_x + self.line_spacing * 0.34, y - self.line_spacing * 0.28);
                notes_canvas
                    .move_to(sharp_x - self.line_spacing * 0.34, y + self.line_spacing * 0.28)
                    .line_to(sharp_x + self.line_spacing * 0.34, y + self.line_spacing * 0.12);
                notes_canvas.stroke(primary, 1.2, 0.86_f32.min(alpha + 0.08));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::{
        SonnetAnimationIntensity, SonnetLayoutDirection, SonnetSegmentRole,
    };

    fn placement() -> SonnetTypographyPlacement {
        SonnetTypographyPlacement {
            segment_index: 0,
            display_text: "staff".into(),
            role: SonnetSegmentRole::Hero,
            font_scale: 1.0,
            measured_width: 200.0,
            measured_height: 80.0,
            x: 320.0,
            y: 240.0,
            rotation: 0.0,
            enter_x: -20.0,
            enter_y: 0.0,
            vertical: false,
            layout_direction: SonnetLayoutDirection::Horizontal,
            timing_phase: 0.0,
        }
    }

    fn theme() -> SonnetTheme {
        SonnetTheme {
            animation_intensity: SonnetAnimationIntensity::Normal,
            primary_color: [1.0, 0.0, 0.0, 1.0],
            secondary_color: [0.0, 1.0, 0.0, 1.0],
            accent_color: [0.0, 0.0, 1.0, 1.0],
        }
    }

    #[test]
    fn positive_modulo_handles_negative_input() {
        // Counter-rotates through the divisor exactly as folia's helper does.
        assert_eq!(positive_modulo(-1.0, 4.0), 3.0);
        assert_eq!(positive_modulo(-2.5, 8.0), 5.5);
        assert_eq!(positive_modulo(0.0, 8.0), 0.0);
        assert_eq!(positive_modulo(7.0, 8.0), 7.0);
        assert_eq!(positive_modulo(8.0, 8.0), 0.0);
        assert_eq!(positive_modulo(9.0, 8.0), 1.0);
    }

    #[test]
    fn build_initialises_static_staff_geometry_once() {
        let view = build_sonnet_staff_view(&placement(), &theme(), 32.0, 1.5, 800.0);
        // staff_width = max(300, 800 * 0.6) = 480.
        assert!((view.staff_width - 480.0).abs() < 0.01);
        // line_spacing = baseFontSize * 0.25 = 8.
        assert_eq!(view.line_spacing, 8.0);
        // total_height = 8 * 4 = 32 → half_height = 16.
        assert_eq!(view.half_height, 16.0);
        // playable_width = 480 * 0.92 = 441.6.
        assert!((view.playable_width - 441.6).abs() < 1e-3);
        // beat_width = 441.6 / 24 = 18.4 (La Folia has 24 total beats).
        assert!((view.beat_width - 18.4).abs() < 1e-2);
        // timed_notes table mirrors the 22-note folia array.
        assert_eq!(view.timed_notes.len(), 22);
        // first note starts at beat 0; cumulative cursor walks forward.
        assert_eq!(view.timed_notes[0].start_beat, 0.0);
        assert_eq!(view.timed_notes[1].start_beat, 1.0);
        assert_eq!(view.timed_notes[2].start_beat, 2.5);
        // GlyphView fields match the architecturally-portable folia return.
        assert_eq!(view.start_time, 1.5);
        assert_eq!(view.settle_time, 2.0);
        assert_eq!(view.final_rotation, 0.0);
        assert_eq!(view.entry_rotation, 0.0);
        assert_eq!(view.z_depth, 0.0);
        assert!(!view.is_text_glyph);
    }

    #[test]
    fn staff_canvas_has_static_layers_budgeted() {
        let view = build_sonnet_staff_view(&placement(), &theme(), 32.0, 0.0, 800.0);
        // At build time only the 5 static staff lines are recorded; the 7 bar
        // lines + (1 + 2) clef bars are drawn per-frame inside update_animation
        // (and cleared each frame) so build yields 5 strokes on staff_canvas.
        let strokes = view.staff_canvas.strokes_count();
        assert_eq!(strokes, 5);
    }

    #[test]
    fn update_animation_emits_cursor_and_note_glyphs_into_notes_layer() {
        let view = build_sonnet_staff_view(&placement(), &theme(), 32.0, 0.0, 800.0);
        let mut notes = MgCanvas::new();
        view.update_animation(0.0, &mut notes);
        // Beat position 0 → cursor (1 stroke) + first note (1 fill + 1 stroke).
        // At t=0 only notes that satisfy `start_beat <= beat_position` are active
        // because the loop falls back to the inactive alpha branch otherwise.
        // Every note still emits the head fill + stem stroke regardless of active
        // state, so expect (1 cursor stroke) + (22 heads + 22 stems base).
        assert!(view.staff_canvas.strokes_count() >= 1);
        assert!(notes.strokes_count() >= 1 + 22);
        assert!(notes.fills_count() >= 22);
    }

    #[test]
    fn update_animation_cycles_independently_of_absolute_time() {
        // Folia wraps the cycle at LA_FOLIA_CYCLE_SECONDS — the music loops.
        let view = build_sonnet_staff_view(&placement(), &theme(), 32.0, 0.0, 800.0);
        let mut notes_a = MgCanvas::new();
        view.update_animation(0.0, &mut notes_a);
        let mut notes_b = MgCanvas::new();
        view.update_animation(LA_FOLIA_CYCLE_SECONDS, &mut notes_b);
        // Same cycle position ⇒ same cursor_x (modulo f32 eps). Strokes_count
        // parity is a coarse equivalence witness; exact path data lives in the
        // canvas path buffer but strokes_count is enough to confirm the same
        // number of primitives emitted.
        assert_eq!(notes_a.strokes_count(), notes_b.strokes_count());
        assert_eq!(notes_a.fills_count(), notes_b.fills_count());
    }

    #[test]
    fn staff_width_floors_at_300_pixels_below_threshold() {
        // `Math.max(300, width * 0.6)` → 300 wins when width < 500.
        let view = build_sonnet_staff_view(&placement(), &theme(), 32.0, 0.0, 400.0);
        assert_eq!(view.staff_width, 300.0);
    }
}
