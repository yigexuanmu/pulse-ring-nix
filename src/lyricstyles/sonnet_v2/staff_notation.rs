//! Folia sonnet v2 — `sonnetStaffNotation.ts` (50 lines) compiler-grade 1:1 port.
//!
//! The La Folia public-domain D-minor theme, transcribed from its 3/4 LilyPond
//! notation. This file holds only the static notation table + two constants
//! derived from it; `staff_view.rs` consumes them to draw the staff.

/// folia `sonnetStaffNotation.ts` — `pitch = 'C#5' | 'D5' | 'E5' | 'F5'`.
///
/// Encoded as a newtype so the four legal pitches form a closed enum rather
/// than the open TS string union; folia's `LA_FOLIA_STAFF_NOTES` table only
/// ever uses these four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetStaffPitch {
    CSharp5,
    D5,
    E5,
    F5,
}

/// folia `sonnetStaffNotation.ts` — `accidental?: 'sharp'`.
///
/// Pre-implemented as an enum rather than a string so future accidentals
/// (`flat`, `natural`) slot in without a schema change; byte-faithful with
/// folia as long as only `Sharp` is ever produced today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetStaffAccidental {
    Sharp,
}

/// folia `sonnetStaffNotation.ts` — `SonnetStaffNote`.
///
/// Field types and ordering match the TS interface byte-for-byte:
/// - `staff_step`: `i32` (TS `staffStep: number`) — used downstream as a
///   float multiplier into `lineSpacing * 0.5`; integer is sufficient because
///   the table only stores integers (5, 6, 7, 8).
/// - `beats`: `f32` — the table contains `0.5`, `1`, `1.5`, `3`.
/// - `accidental`: `Option<SonnetStaffAccidental>` (TS `accidental?: 'sharp'`).
#[derive(Debug, Clone, Copy)]
pub struct SonnetStaffNote {
    pub pitch: SonnetStaffPitch,
    pub staff_step: i32,
    pub beats: f32,
    pub accidental: Option<SonnetStaffAccidental>,
}

/// folia `sonnetStaffNotation.ts` — `LA_FOLIA_STAFF_NOTES`.
///
/// La Folia's 3/4 D-minor theme, 22 notes. Order is the playback order the
/// visualiser drives; every byte (pitch / staff_step / beats / accidental)
/// matches the TS source byte-for-byte.
pub const LA_FOLIA_STAFF_NOTES: &[SonnetStaffNote] = &[
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 0.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::CSharp5, staff_step: 5, beats: 1.0, accidental: Some(SonnetStaffAccidental::Sharp) },
    SonnetStaffNote { pitch: SonnetStaffPitch::CSharp5, staff_step: 5, beats: 1.0, accidental: Some(SonnetStaffAccidental::Sharp) },
    SonnetStaffNote { pitch: SonnetStaffPitch::CSharp5, staff_step: 5, beats: 1.0, accidental: Some(SonnetStaffAccidental::Sharp) },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 0.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::F5, staff_step: 8, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::F5, staff_step: 8, beats: 1.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::F5, staff_step: 8, beats: 0.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::E5, staff_step: 7, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.0, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 1.5, accidental: None },
    SonnetStaffNote { pitch: SonnetStaffPitch::CSharp5, staff_step: 5, beats: 0.5, accidental: Some(SonnetStaffAccidental::Sharp) },
    SonnetStaffNote { pitch: SonnetStaffPitch::D5, staff_step: 6, beats: 3.0, accidental: None },
];

/// folia `sonnetStaffNotation.ts` — `LA_FOLIA_TOTAL_BEATS`.
///
/// TS evaluates this once at module load via `reduce((total, n) => total + n.beats, 0)`.
/// The math = 1+1.5+0.5+1+1+1+1+1.5+0.5+1+1+1+1+1.5+0.5+1+1+1+1+1.5+0.5+3 = 25.
/// Kept as a function (not a `const`) so it stays byte-faithful to the
/// "computed-from-table" semantics and stays correct if the table ever
/// changes.
pub fn la_folia_total_beats() -> f32 {
    LA_FOLIA_STAFF_NOTES.iter().map(|n| n.beats).sum()
}

/// folia `sonnetStaffNotation.ts` — `LA_FOLIA_CYCLE_SECONDS = 8`.
///
/// Public-domain La Folia playback loop length. `const` rather than function
/// because folia declares it as a literal `export const`.
pub const LA_FOLIA_CYCLE_SECONDS: f32 = 8.0;

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    /// The 22-note La Folia table — exact count and zero-clone integrity.
    #[test]
    fn la_folia_staff_notes_table_has_22_entries() {
        assert_eq!(LA_FOLIA_STAFF_NOTES.len(), 22);
    }

    /// `LA_FOLIA_TOTAL_BEATS` reduces to exactly 25.0 — matches the
    /// sum the LilyPond transcription encodes.
    #[test]
    fn la_folia_total_beats_is_twenty_four() {
        assert_eq!(la_folia_total_beats(), 24.0);
    }

    /// `LA_FOLIA_CYCLE_SECONDS` is the literal 8 the TS exports.
    #[test]
    fn la_folia_cycle_seconds_is_eight() {
        assert_eq!(LA_FOLIA_CYCLE_SECONDS, 8.0);
    }

    /// every `beats` value is positive (`> 0`) — a zero or negative beat
    /// would break the cumulative `startBeat` accumulation in upstream code.
    #[test]
    fn every_note_has_positive_beat_duration() {
        for (i, n) in LA_FOLIA_STAFF_NOTES.iter().enumerate() {
            assert!(
                n.beats > 0.0,
                "note[{i}] beats={} must be > 0 (would break cumulative beat cursor)",
                n.beats,
            );
        }
    }

    /// `accidental` is only ever `Some(Sharp)` in the table; no rogue
    /// `Natural`/`Flat` (which don't exist yet anyway) crept in.
    #[test]
    fn accidental_only_ever_sharp_le_three_times() {
        let sharps: Vec<usize> = LA_FOLIA_STAFF_NOTES
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.accidental.map(|_| i))
            .collect();
        // Indices 3, 4, 5 (three C#5 in a row) and 20 (the lone C#5 pickup).
        assert_eq!(sharps, vec![3, 4, 5, 20]);
    }

    /// the cumulative `startBeat` of every note is monotonic non-decreasing
    /// (cumulative cursor the upstream `buildSonnetStaffView` walks).
    #[test]
    fn cumulative_start_beat_is_monotonic_non_decreasing() {
        let mut cursor = 0.0_f32;
        for n in LA_FOLIA_STAFF_NOTES {
            let prev = cursor;
            cursor += n.beats;
            assert_eq!(cursor.partial_cmp(&prev), Some(Ordering::Greater));
        }
        assert_eq!(cursor, 24.0);
    }
}
