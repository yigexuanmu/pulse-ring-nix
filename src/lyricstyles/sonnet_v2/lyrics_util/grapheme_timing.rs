//! `utils/lyrics/graphemeTiming.ts` (154 lines) — compiler-grade 1:1 Rust port.
//!
//! Builds parser-derived grapheme timing without owning visualizer animation
//! curves. Pure functions; the only external dep is an `Intl.Segmenter('grapheme')`
//! equivalent — Rust uses `unicode_segmentation::UnicodeSegmentation` (UAX #29
//! extended grapheme clusters), which is the same algorithm `Intl.Segmenter` ships.
//!
//! Time units are seconds (matches folia `Line.startTime`/`endTime`).

use unicode_segmentation::UnicodeSegmentation;

use crate::lyricstyles::sonnet_v2::types::{
    GraphemeTiming, Line, Syllable, Word,
};

/// folia `graphemeTiming.ts` — `splitLyricGraphemes`.
///
/// Returns empty `Vec` for empty input. Otherwise splits `text` into extended
/// grapheme clusters via UAX #29 (Rust `UnicodeSegmentation::graphemes(true)` ↔
/// folia `Intl.Segmenter({ granularity: 'grapheme' })`).
pub fn split_lyric_graphemes(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.graphemes(true).map(String::from).collect()
}

/// folia `graphemeTiming.ts` — `buildEvenGraphemeTimings`.
///
/// Splits `text` into graphemes and assigns each an equal slice of the
/// `[startTime, endTime]` window. The last grapheme snaps to `endTime` exactly
/// (avoids floating-point drift accumulating at the boundary). `wordIndex`
/// becomes `Some` only when passed a finite `usize`; `None` mirrors the TS
/// `undefined` spread (`...(typeof wordIndex === 'number' ? { wordIndex } : {})`).
fn build_even_grapheme_timings(
    text: &str,
    start_time: f64,
    end_time: f64,
    word_index: Option<usize>,
) -> Vec<GraphemeTiming> {
    let graphemes = split_lyric_graphemes(text);
    if graphemes.is_empty() {
        return Vec::new();
    }
    let duration = (end_time - start_time).max(0.0);
    let unit = duration / graphemes.len() as f64;

    graphemes
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let start = start_time + unit * i as f64;
            let end = if i == graphemes.len() - 1 {
                end_time
            } else {
                start_time + unit * (i as f64 + 1.0)
            };
            GraphemeTiming {
                char: ch.clone(),
                start_time: start,
                end_time: end,
                word_index,
            }
        })
        .collect()
}

/// folia `graphemeTiming.ts` — `buildWordGraphemeTimings`.
///
/// If `word.syllables` is empty, distributes the word window evenly across the
/// word's graphemes. Otherwise splits by syllable: each syllable's graphemes get
/// an even share of that syllable's window (and all inherit the parent `wordIndex`).
pub fn build_word_grapheme_timings(word: &Word, word_index: Option<usize>) -> Vec<GraphemeTiming> {
    if word.syllables.is_empty() {
        return build_even_grapheme_timings(&word.text, word.start_time, word.end_time, word_index);
    }
    word.syllables
        .iter()
        .flat_map(|s| build_syllable_grapheme_timings(s, word_index))
        .collect()
}

fn build_syllable_grapheme_timings(
    syllable: &Syllable,
    word_index: Option<usize>,
) -> Vec<GraphemeTiming> {
    build_even_grapheme_timings(
        &syllable.text,
        syllable.start_time,
        syllable.end_time,
        word_index,
    )
}

/// folia `graphemeTiming.ts` — `findGraphemeSequence`.
///
/// Naive substring search on the grapheme-`Vec` slotates the `target` run inside
/// `source` starting at `fromIndex` or later. Returns -1 on no match. Empty
/// `target` short-circuits to `fromIndex`, matching the JS branch.
fn find_grapheme_sequence(
    source: &[String],
    target: &[String],
    from_index: usize,
) -> i64 {
    if target.is_empty() {
        return from_index as i64;
    }
    if source.len() < target.len() {
        return -1;
    }
    let last_start = source.len() - target.len();
    let mut index = from_index;
    while index <= last_start {
        let mut matched = true;
        for ti in 0..target.len() {
            if source[index + ti] != target[ti] {
                matched = false;
                break;
            }
        }
        if matched {
            return index as i64;
        }
        index += 1;
    }
    -1
}

/// folia `graphemeTiming.ts` — `buildLineGraphemeTimeline`.
///
/// Maps word-level timing back onto the full displayed line — filling gaps (spaces,
/// punctuation absent from parser words) with stuck-time graphemes derived from the
/// following word's `startTime`. The timeline length equals the line's grapheme
/// count; every slot is guaranteed populated.
///
/// Step-by-step (1:1 with the TS):
/// 1. `lineGraphemes = splitLyricGraphemes(line.fullText)`; bail if empty.
/// 2. If `line.words` is empty, even-distribute over `[line.startTime, line.endTime]`.
/// 3. Otherwise walk `line.words`:
///    a. split the word's graphemes; skip empty words;
///    b. `matchedStart = findGraphemeSequence(line, wordGraphemes, cursor)`; if not
///       found, fall back to `cursor` (TS `matchedStart >= 0 ? matchedStart : cursor`);
///    c. clamp `end = min(start + wordGraphemes.len, lineGraphemes.len)`;
///    d. fill `cursor..start` gap slots with stuck-time graphemes at `word.startTime`
///       (the gap inherits the upcoming word's start both for start and end);
///    e. for each `0..(end-start)`, prefer the syllable/word timing at that local
///       index; if missing (`wordTimings[localIndex]` is undefined in JS), fall
///       back to a single-grapheme even distribution for that one character;
///    f. overwrite `timeline[start+local]` slot's text with the line grapheme (so
///       the rendered string is the line text, not the word text — they may differ
///       for normalisation edge cases), keep the syllable-computed times + wordIndex;
///    g. track `lastResolvedTime = max(lastResolvedTime, timing.endTime)`;
///    h. `cursor = max(cursor, end)` (skip already-filled slots).
/// 4. Fill any remaining unfilled slots with stuck-time at `lastResolvedTime`.
pub fn build_line_grapheme_timeline(line: &Line) -> Vec<GraphemeTiming> {
    let line_graphemes = split_lyric_graphemes(&line.full_text);
    if line_graphemes.is_empty() {
        return Vec::new();
    }
    if line.words.is_empty() {
        return build_even_grapheme_timings(&line.full_text, line.start_time, line.end_time, None);
    }

    let n = line_graphemes.len();
    let mut timeline: Vec<Option<GraphemeTiming>> = (0..n).map(|_| None).collect();
    let mut cursor: usize = 0;
    let mut last_resolved_time = line.start_time;

    for (word_index, word) in line.words.iter().enumerate() {
        let word_graphemes = split_lyric_graphemes(&word.text);
        if word_graphemes.is_empty() {
            continue;
        }
        let matched_start = find_grapheme_sequence(&line_graphemes, &word_graphemes, cursor);
        let start = if matched_start >= 0 {
            matched_start as usize
        } else {
            cursor
        };
        let end = (start + word_graphemes.len()).min(n);

        // (d) fill the gap slots `cursor..start` with stuck-time graphemes that
        // all inherit the upcoming word's start for both start and end.
        for gap in cursor..start {
            timeline[gap] = Some(GraphemeTiming {
                char: line_graphemes[gap].clone(),
                start_time: word.start_time,
                end_time: word.start_time,
                word_index: None,
            });
        }

        // (e)/(f) prefer per-syllable/word timing; fall back to a synthesized
        // single-grapheme even distribution for that local index when the inner
        // timing vec is short (defensive — normal flow keeps `local` in bounds).
        let word_timings = build_word_grapheme_timings(word, Some(word_index));
        for local in 0..(end - start) {
            let timing = if let Some(t) = word_timings.get(local) {
                t.clone()
            } else {
                let fallback_char = word_graphemes.get(local).cloned().unwrap_or_default();
                build_even_grapheme_timings(
                    &fallback_char,
                    word.start_time,
                    word.end_time,
                    Some(word_index),
                )
                .into_iter()
                .next()
                .unwrap_or(GraphemeTiming {
                    char: fallback_char,
                    start_time: word.start_time,
                    end_time: word.end_time,
                    word_index: Some(word_index),
                })
            };

            let slot = start + local;
            timeline[slot] = Some(GraphemeTiming {
                char: line_graphemes[slot].clone(),
                start_time: timing.start_time,
                end_time: timing.end_time,
                word_index: timing.word_index,
            });
            last_resolved_time = last_resolved_time.max(timing.end_time);
        }

        cursor = cursor.max(end);
    }

    // (4) Fill any remaining unfilled slots with stuck-time at `lastResolvedTime`.
    for index in 0..n {
        if timeline[index].is_none() {
            timeline[index] = Some(GraphemeTiming {
                char: line_graphemes[index].clone(),
                start_time: last_resolved_time,
                end_time: last_resolved_time,
                word_index: None,
            });
        }
    }

    // Safety: every slot is now `Some` — the trailing loop guarantees it.
    timeline.into_iter().map(|t| t.unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::{Line, Word};

    fn word(text: &str, start: f64, end: f64) -> Word {
        Word {
            text: text.to_string(),
            start_time: start,
            end_time: end,
            syllables: Vec::new(),
        }
    }

    fn line(full_text: &str, words: Vec<Word>, start: f64, end: f64) -> Line {
        Line {
            words,
            start_time: start,
            end_time: end,
            full_text: full_text.to_string(),
            render_hints: None,
        }
    }

    #[test]
    fn split_lyric_graphemes_handles_empty_and_extended_clusters() {
        assert!(split_lyric_graphemes("").is_empty());
        // CRLF is a single grapheme cluster per UAX #29; family emoji ("👨‍👩‍👧") is one.
        assert_eq!(split_lyric_graphemes("a\r\nb"), vec!["a", "\r\n", "b"]);
    }

    #[test]
    fn build_even_grapheme_timings_distributes_evenly_with_final_snap() {
        let timings = build_even_grapheme_timings("abcd", 1.0, 5.0, Some(0));
        assert_eq!(timings.len(), 4);
        // duration = 4, unit = 1.0; last snaps to end_time exactly.
        assert!((timings[0].start_time - 1.0).abs() < 1e-12);
        assert!((timings[3].end_time - 5.0).abs() < 1e-12);
        assert_eq!(timings[2].word_index, Some(0));
    }

    #[test]
    fn build_line_grapheme_timeline_fills_gap_from_following_word_start() {
        // line text "ab cd" with one word "ab" at [0,2] and "cd" at [3,5].
        // The middle space slot (index 2) inherits cd.startTime=3 for both ends.
        let l = line(
            "ab cd",
            vec![word("ab", 0.0, 2.0), word("cd", 3.0, 5.0)],
            0.0,
            5.0,
        );
        let tl = build_line_grapheme_timeline(&l);
        assert_eq!(tl.len(), 5);
        assert_eq!(tl[0].char, "a");
        assert_eq!(tl[3].char, "c");
        // gap slot
        assert_eq!(tl[2].char, " ");
        assert!((tl[2].start_time - 3.0).abs() < 1e-12);
        assert!((tl[2].end_time - 3.0).abs() < 1e-12);
        assert_eq!(tl[2].word_index, None);
        // word slots word_index attached
        assert_eq!(tl[0].word_index, Some(0));
        assert_eq!(tl[3].word_index, Some(1));
    }

    #[test]
    fn build_line_grapheme_timeline_empty_words_even_distributes_line_window() {
        let l = line("あいう", vec![], 0.0, 3.0);
        let tl = build_line_grapheme_timeline(&l);
        assert_eq!(tl.len(), 3);
        assert!((tl[0].start_time - 0.0).abs() < 1e-12);
        assert!((tl[2].end_time - 3.0).abs() < 1e-12);
        assert_eq!(tl[0].word_index, None);
    }

    #[test]
    fn find_grapheme_sequence_simple_run() {
        let src: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        let tgt: Vec<String> = ["c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(find_grapheme_sequence(&src, &tgt, 0), 2);
        // from_index after the only match → -1
        assert_eq!(find_grapheme_sequence(&src, &tgt, 3), -1);
        // empty target short-circuits
        assert_eq!(find_grapheme_sequence(&src, &[], 4), 4);
    }
}
