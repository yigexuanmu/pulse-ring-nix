//! `utils/lyrics/renderHints.ts` (243 lines) — compiler-grade 1:1 Rust port.
//!
//! Derives per-line visualisation hints (`timingClass`, `renderEndTime`,
//! transition/reveal modes) from start/end/word timing. Pure functions; no
//! PIXI / DOM dependency.
//!
//! # Faithfulness notes
//! Times are in seconds, matching folia `number`. The TS exposes structural
//! interfaces `RenderHintLineLike` / `RenderHintWordLike` / `RenderHintLyricDataLike`
//! so the helpers are generic over any lyric payload shape; Rust ports against the
//! concrete [`Line`](crate::lyricstyles::sonnet_v2::types::Line) (which owns
//! `render_hints: Option<LineRenderHints>`). The `migrate*` family reports
//! [`MigrationResult::changed`] in place of the TS `{ value, changed }` pair —
//! Rust callers mutate the owned `Line`s in place (no immutable spread needed).

use crate::lyricstyles::sonnet_v2::types::{
    Line, LineRenderHints, LineTimingClass, LineTransitionMode, LineTransitionTiming, WordRevealMode,
};

/// folia `renderHints.ts` — `MICRO_LINE_DURATION_THRESHOLD = 0.10`.
pub const MICRO_LINE_DURATION_THRESHOLD: f64 = 0.10;
/// folia `renderHints.ts` — `SHORT_LINE_DURATION_THRESHOLD = 0.18`.
pub const SHORT_LINE_DURATION_THRESHOLD: f64 = 0.18;
/// folia `renderHints.ts` — `MICRO_LINE_RENDER_FLOOR = 0.067`.
pub const MICRO_LINE_RENDER_FLOOR: f64 = 0.067;

/// Outcome of a render-hints migration step — mirrors the TS `MigrationResult<T>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationResult<T> {
    pub value: T,
    pub changed: bool,
}

/// folia `renderHints.ts` — `clamp(value, min, max)`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// folia `renderHints.ts` — `getLastWordEndTime(line)`.
///
/// Returns the `end_time` of the last word, falling back to the line's
/// `end_time` when there are no words.
fn get_last_word_end_time(line: &Line) -> f64 {
    line.words
        .last()
        .map(|w| w.end_time)
        .unwrap_or(line.end_time)
}

/// folia `renderHints.ts` — `getLineTransitionTiming`.
pub fn get_line_transition_timing(
    raw_duration: f64,
    line_transition_mode: LineTransitionMode,
    word_reveal_mode: WordRevealMode,
) -> LineTransitionTiming {
    if line_transition_mode == LineTransitionMode::None {
        return LineTransitionTiming {
            enter_duration: 0.0,
            exit_duration: 0.0,
            line_pass_hold: 0.0,
        };
    }

    if line_transition_mode == LineTransitionMode::Fast {
        return LineTransitionTiming {
            enter_duration: clamp(raw_duration * 0.45, 0.045, 0.06),
            exit_duration: clamp(raw_duration * 0.22, 0.03, 0.04),
            line_pass_hold: if word_reveal_mode == WordRevealMode::Instant {
                0.0
            } else {
                0.03
            },
        };
    }

    LineTransitionTiming {
        enter_duration: 0.42_f64.min((raw_duration.max(0.12) * 0.34).max(0.22)),
        exit_duration: 0.32_f64.min((raw_duration.max(0.12) * 0.18).max(0.18)),
        line_pass_hold: if word_reveal_mode == WordRevealMode::Instant {
            0.0
        } else {
            0.06
        },
    }
}

/// folia `renderHints.ts` — `getTimingClass`.
fn get_timing_class(raw_duration: f64) -> LineTimingClass {
    if raw_duration < MICRO_LINE_DURATION_THRESHOLD {
        return LineTimingClass::Micro;
    }
    if raw_duration < SHORT_LINE_DURATION_THRESHOLD {
        return LineTimingClass::Short;
    }
    LineTimingClass::Normal
}

/// folia `renderHints.ts` — `getLineTransitionMode`.
fn get_line_transition_mode(timing_class: LineTimingClass) -> LineTransitionMode {
    match timing_class {
        LineTimingClass::Micro => LineTransitionMode::None,
        LineTimingClass::Short => LineTransitionMode::Fast,
        LineTimingClass::Normal => LineTransitionMode::Normal,
    }
}

/// folia `renderHints.ts` — `getWordRevealMode`.
fn get_word_reveal_mode(timing_class: LineTimingClass) -> WordRevealMode {
    match timing_class {
        LineTimingClass::Micro => WordRevealMode::Instant,
        LineTimingClass::Short => WordRevealMode::Fast,
        LineTimingClass::Normal => WordRevealMode::Normal,
    }
}

/// folia `renderHints.ts` — `buildLineRenderEndTime`.
fn build_line_render_end_time(
    line: &Line,
    raw_duration: f64,
    line_transition_mode: LineTransitionMode,
    word_reveal_mode: WordRevealMode,
) -> f64 {
    if line_transition_mode == LineTransitionMode::None {
        return line.end_time.max(line.start_time + MICRO_LINE_RENDER_FLOOR);
    }

    let transition_timing =
        get_line_transition_timing(raw_duration, line_transition_mode, word_reveal_mode);
    let line_pass_start =
        get_last_word_end_time(line).max(line.start_time) + transition_timing.line_pass_hold;
    let exit_start = if line_transition_mode == LineTransitionMode::Fast {
        (line.start_time + transition_timing.enter_duration + 0.01)
            .max(line_pass_start)
            .max(line.end_time - transition_timing.exit_duration)
    } else {
        line_pass_start.max(line.end_time - transition_timing.exit_duration)
    };

    line.end_time.max(exit_start + transition_timing.exit_duration)
}

/// folia `renderHints.ts` overload — `buildLineRenderHints(line: RenderHintLineLike)`.
pub fn build_line_render_hints(line: &Line) -> LineRenderHints {
    build_line_render_hints_for_window(&line.start_time, &line.end_time, Some(line))
}

/// folia `renderHints.ts` overload — `buildLineRenderHints(startTime, endTime)`.
///
/// When `line` is `Some`, `getLastWordEndTime` / `startTime` / `endTime` are read
/// from it (the `(startTime, endTime)` overload constructs a synthetic line with
/// no words, so the last-word end falls back to `endTime`).
pub fn build_line_render_hints_for_window(
    start_time: &f64,
    end_time: &f64,
    line: Option<&Line>,
) -> LineRenderHints {
    let line_ref = line.cloned().unwrap_or(Line {
        words: Vec::new(),
        start_time: *start_time,
        end_time: *end_time,
        full_text: String::new(),
        render_hints: None,
    });
    let line = &line_ref;

    let raw_duration = (line.end_time - line.start_time).max(0.0);
    let timing_class = get_timing_class(raw_duration);
    let line_transition_mode = get_line_transition_mode(timing_class);
    let word_reveal_mode = get_word_reveal_mode(timing_class);

    LineRenderHints {
        raw_duration,
        timing_class,
        render_end_time: build_line_render_end_time(
            line,
            raw_duration,
            line_transition_mode,
            word_reveal_mode,
        ),
        line_transition_mode,
        word_reveal_mode,
    }
}

/// folia `renderHints.ts` — `getLineRenderHints(line)`.
///
/// Returns `line.render_hints` if already cached, otherwise builds fresh hints.
/// `None` input (TS `null`/`undefined`) returns `None`.
pub fn get_line_render_hints(line: Option<&Line>) -> Option<LineRenderHints> {
    let line = line?;
    Some(line.render_hints.unwrap_or_else(|| build_line_render_hints(line)))
}

/// folia `renderHints.ts` — `getLineRenderEndTime(line)`.
///
/// Returns `f64::NEG_INFINITY` for `None` input (TS `Number.NEGATIVE_INFINITY`).
pub fn get_line_render_end_time(line: Option<&Line>) -> f64 {
    let Some(line) = line else {
        return f64::NEG_INFINITY;
    };
    get_line_render_hints(Some(line))
        .map(|h| h.render_end_time)
        .unwrap_or(line.end_time)
}

/// folia `renderHints.ts` — `hasExpectedRenderHints(line, expected)`.
fn has_expected_render_hints(line: &Line, expected: &LineRenderHints) -> bool {
    line.render_hints
        .as_ref()
        .map(|current| {
            current.raw_duration == expected.raw_duration
                && current.timing_class == expected.timing_class
                && current.render_end_time == expected.render_end_time
                && current.line_transition_mode == expected.line_transition_mode
                && current.word_reveal_mode == expected.word_reveal_mode
        })
        .unwrap_or(false)
}

/// folia `renderHints.ts` — `migrateLyricLinesRenderHints(lines)`.
///
/// Computes the expected hints for each line; if the cached value already matches
/// bit-for-bit, the line is left untouched; otherwise `render_hints` is updated.
/// Returns a [`MigrationResult`] whose `changed` flag is true when at least one
/// line was updated. In Rust, mutation happens in place (the TS spread copy is
/// unnecessary because `Line` owns `render_hints`).
pub fn migrate_lyric_lines_render_hints(lines: &mut [Line]) -> MigrationResult<()> {
    let mut changed = false;
    for line in lines.iter_mut() {
        let expected = build_line_render_hints(line);
        if !has_expected_render_hints(line, &expected) {
            line.render_hints = Some(expected);
            changed = true;
        }
    }
    MigrationResult {
        value: (),
        changed,
    }
}

/// folia `renderHints.ts` — `annotateLyricLines(lines)`.
///
/// Returns the (mutated in place) lines; sugar over `migrate_lyric_lines_render_hints`.
pub fn annotate_lyric_lines(lines: &mut [Line]) {
    migrate_lyric_lines_render_hints(lines);
}

/// folia `renderHints.ts` — `ensureLyricLinesRenderHints(lines)`.
pub fn ensure_lyric_lines_render_hints(lines: &mut [Line]) {
    migrate_lyric_lines_render_hints(lines);
}

/// folia `renderHints.ts` — `migrateLyricDataRenderHints(lyrics)`.
///
/// `lyrics` is a lyric-data wrapper carrying `lines: Line[]`. The Rust port takes
/// a mutable `lines` slice directly (the outer wrapper carries no other state the
/// migrate family reads or writes). `None` input yields `{ value: None, changed: false }`.
pub fn migrate_lyric_data_render_hints(lines: Option<&mut [Line]>) -> MigrationResult<()> {
    let Some(lines) = lines else {
        return MigrationResult {
            value: (),
            changed: false,
        };
    };
    migrate_lyric_lines_render_hints(lines)
}

/// folia `renderHints.ts` — `ensureLyricDataRenderHints(lyrics)`.
pub fn ensure_lyric_data_render_hints(lines: Option<&mut [Line]>) {
    migrate_lyric_data_render_hints(lines);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::{Line, Word};

    fn word(end_time: f64) -> Word {
        Word {
            text: "x".to_string(),
            start_time: 0.0,
            end_time,
            syllables: Vec::new(),
        }
    }

    fn line(start: f64, end: f64, words: Vec<Word>) -> Line {
        Line {
            words,
            start_time: start,
            end_time: end,
            full_text: "".to_string(),
            render_hints: None,
        }
    }

    #[test]
    fn timing_class_thresholds_are_exact() {
        // < 0.10 → micro, [0.10, 0.18) → short, >= 0.18 → normal
        assert_eq!(get_timing_class(0.099), LineTimingClass::Micro);
        assert_eq!(get_timing_class(0.10), LineTimingClass::Short);
        assert_eq!(get_timing_class(0.179), LineTimingClass::Short);
        assert_eq!(get_timing_class(0.18), LineTimingClass::Normal);
        assert_eq!(get_timing_class(2.0), LineTimingClass::Normal);
    }

    #[test]
    fn transition_mode_none_yields_zero_durations_and_micro_floor_floor() {
        let l = line(1.0, 1.05, vec![]); // micro → none
        let hints = build_line_render_hints(&l);
        assert_eq!(hints.line_transition_mode, LineTransitionMode::None);
        let tt = get_line_transition_timing(hints.raw_duration, LineTransitionMode::None, WordRevealMode::Instant);
        assert_eq!(tt, LineTransitionTiming { enter_duration: 0.0, exit_duration: 0.0, line_pass_hold: 0.0 });
        // renderEndTime = max(end, start + MICRO_LINE_RENDER_FLOOR)
        assert!((hints.render_end_time - (1.0 + MICRO_LINE_RENDER_FLOOR)).abs() < 1e-12);
    }

    #[test]
    fn build_line_render_end_time_fast_uses_enter_plus_delta_when_short() {
        // short window, no words → fast transition; passStart uses end_time as
        // last-word-end (falls back when words is empty).
        let l = line(0.0, 0.12, vec![]);
        let hints = build_line_render_hints(&l);
        assert_eq!(hints.line_transition_mode, LineTransitionMode::Fast);
        let tt = get_line_transition_timing(0.12, LineTransitionMode::Fast, WordRevealMode::Fast);
        let pass_start = 0.12_f64.max(0.0) + tt.line_pass_hold; // last-word-end=end_time
        let exit_start = (0.0 + tt.enter_duration + 0.01)
            .max(pass_start)
            .max(0.12 - tt.exit_duration);
        let expected = 0.12_f64.max(exit_start + tt.exit_duration);
        assert!((hints.render_end_time - expected).abs() < 1e-12);
    }

    #[test]
    fn last_word_end_time_extends_render_end_time_for_normal_line() {
        let l = line(0.0, 1.0, vec![word(0.9)]);
        let hints = build_line_render_hints(&l);
        // passStart uses max(lastWordEnd=0.9, start=0) + linePassHold=0.06 = 0.96
        let tt = get_line_transition_timing(1.0, LineTransitionMode::Normal, WordRevealMode::Normal);
        assert!((tt.line_pass_hold - 0.06).abs() < 1e-12);
        let pass_start = 0.9_f64 + 0.06;
        let exit_start = pass_start.max(1.0 - tt.exit_duration);
        let expected = 1.0_f64.max(exit_start + tt.exit_duration);
        assert!((hints.render_end_time - expected).abs() < 1e-12);
    }

    #[test]
    fn get_line_render_end_time_neg_infinity_for_none() {
        assert!(get_line_render_end_time(None).is_infinite());
        assert!(get_line_render_end_time(None).is_sign_negative());
    }

    #[test]
    fn migrate_is_noop_when_cached_matches() {
        let mut l = vec![line(0.0, 1.0, vec![])];
        let expected = build_line_render_hints(&l[0]);
        l[0].render_hints = Some(expected);
        let result = migrate_lyric_lines_render_hints(&mut l);
        assert!(!result.changed);
        assert!(l[0].render_hints.is_some());
    }

    #[test]
    fn migrate_populates_when_missing() {
        let mut l = vec![line(0.0, 1.0, vec![])];
        assert!(l[0].render_hints.is_none());
        let result = migrate_lyric_lines_render_hints(&mut l);
        assert!(result.changed);
        let hints = l[0].render_hints.expect("hints should be populated");
        assert_eq!(hints.timing_class, LineTimingClass::Normal);
        assert_eq!(hints.line_transition_mode, LineTransitionMode::Normal);
    }

    #[test]
    fn build_line_render_hints_for_window_no_line_uses_endtime_as_last_word() {
        // (startTime, endTime) overload synthesises a line with no words →
        // getLastWordEndTime falls back to endTime.
        let hints = build_line_render_hints_for_window(&0.0, &1.5, None);
        assert_eq!(hints.timing_class, LineTimingClass::Normal);
        assert!((hints.raw_duration - 1.5).abs() < 1e-12);
    }
}
