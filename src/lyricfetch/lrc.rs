//! LRC / inline-KRC / enhanced-word parser — clean-room port of
//! `lyric_sources.py::parse_lrc` (lines 190-248).
//!
//! Handles an LRC body whose lines may carry plain `[mm:ss.xxx]` timestamps, multiple
//! timestamps per line, `<ss.fff>word` enhanced-word tags, or inline KRC-style
//! `[start,dur]<off,dur>word` runs. Output times are ms integers; durations absent from the
//! source are inferred from the next timed line by [`super::finalize`].

use crate::lyrics::LyricLine;
use regex::{Captures, Regex};
use std::sync::LazyLock;
use super::{clean_text, finalize, number, timestamp_ms};

// module-level regexes (mirror lyric_sources.py lines 21-27)
static TIME_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\]").unwrap()
});
static KRC_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d+),(\d+)\](.*)$").unwrap());
static PREFIX_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:<|\()(\d+),(\d+)(?:,\d+)?(?:>|\))([^<(]*)").unwrap()
});
static SUFFIX_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(.*?)<(\d+),(\d+)(?:,\d+)?>").unwrap());
static QRC_SUFFIX_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(.*?)[(](\d+),(\d+)[)]").unwrap());
static ENHANCED_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(?:(\d+):)?(\d{1,2}(?:[.:]\d{1,3})?)>([^<]*)").unwrap()
});
static META_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\[(ar|al|ti|by|re|ve|length|offset):").unwrap()
});
static CREDIT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(词|曲|作词|作曲|编曲|制作人|lyricist|composer|arranger)\s*[:：]").unwrap()
});
static OFFSET_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[offset:([+-]?\d+)\]").unwrap());
static PREFIX_HEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[<(]\d+,").unwrap());

/// Parse an LRC blob into timed `LyricLine`s. Faithful port of `parse_lrc` (190-248).
pub(crate) fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let offset = OFFSET_TAG
        .captures(&text)
        .map(|c| number(&c[1], 0))
        .unwrap_or(0);
    let mut result: Vec<LyricLine> = Vec::new();

    for raw_line in text.split('\n') {
        let raw = raw_line.trim();
        if raw.is_empty() || META_TAG.is_match(raw) {
            continue;
        }
        if let Some(krc) = KRC_LINE.captures(raw) {
            krc_branch(&krc, offset, &mut result);
            continue;
        }
        time_tag_branch(raw, offset, &mut result);
    }
    finalize(result, 0)
}

/// `parse_plain` (lyric_sources.py:186): split text into untimed lines (time = -1),
/// dropping any whose `clean_text` is empty. Does NOT call `finalize` (matching Python).
pub(crate) fn parse_plain(text: &str) -> Vec<LyricLine> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for raw in normalized.split('\n') {
        let cleaned = clean_text(raw);
        if cleaned.is_empty() {
            continue;
        }
        out.push(LyricLine {
            start_ms: -1,
            duration_ms: 0,
            text: cleaned,
            translation: String::new(),
            romanization: String::new(),
            chars: Vec::new(),
            words: Vec::new(),
            song_part: String::new(),
            block_index: 0,
            chorus_flag: false,
        });
    }
    out
}

/// Inline KRC-styled line `[start,dur]<...>body` (lyric_sources.py:204-227).
fn krc_branch(krc: &Captures, offset: i64, out: &mut Vec<LyricLine>) {
    let start = number(&krc[1], 0);
    let duration = number(&krc[2], 0);
    let body = &krc[3];
    let prefix_heads = PREFIX_HEAD.is_match(body);
    let prefix_words: Vec<Captures> = if prefix_heads {
        PREFIX_WORD.captures_iter(body).collect()
    } else {
        Vec::new()
    };
    let mut absolute_word_times = body.starts_with('(');

    let pieces: Vec<(String, i64, i64)> = if !prefix_words.is_empty() {
        prefix_words
            .iter()
            .map(|w| (w[3].to_string(), number(&w[1], 0), number(&w[2], 0)))
            .collect()
    } else {
        let mut sw: Vec<(String, i64, i64)> = SUFFIX_WORD
            .captures_iter(body)
            .map(|c| (c[1].to_string(), number(&c[2], 0), number(&c[3], 0)))
            .collect();
        if sw.is_empty() {
            sw = QRC_SUFFIX_WORD
                .captures_iter(body)
                .map(|c| (c[1].to_string(), number(&c[2], 0), number(&c[3], 0)))
                .collect();
            absolute_word_times = !sw.is_empty();
        }
        sw
    };

    if !pieces.is_empty() {
        let mut content = String::new();
        let mut chars: Vec<i64> = Vec::new();
        for (word, woff, wdur) in &pieces {
            let len = word.chars().count().max(1) as i64;
            let word_start = if absolute_word_times { *woff } else { start + woff };
            for (i, ch) in word.chars().enumerate() {
                content.push(ch);
                chars.push(word_start + (i as i64) * wdur / len);
            }
        }
        let content_clean = clean_text(&content);
        if !content_clean.is_empty() {
            out.push(LyricLine {
                start_ms: start + offset,
                duration_ms: duration.max(0),
                text: content_clean,
                translation: String::new(),
                romanization: String::new(),
                chars,
                words: Vec::new(),
                song_part: String::new(),
                block_index: 0,
                chorus_flag: false,
            });
            return;
        }
    }
    let body_clean = clean_text(body);
    if !body_clean.is_empty() {
        out.push(LyricLine {
            start_ms: start + offset,
            duration_ms: duration.max(0),
            text: body_clean,
            translation: String::new(),
            romanization: String::new(),
            chars: Vec::new(),
            words: Vec::new(),
            song_part: String::new(),
            block_index: 0,
            chorus_flag: false,
        });
    }
}

/// Plain / multi-timestamp / enhanced-word LRC line (lyric_sources.py:229-247).
fn time_tag_branch(raw: &str, offset: i64, out: &mut Vec<LyricLine>) {
    let tags: Vec<Captures> = TIME_TAG.captures_iter(raw).collect();
    if tags.is_empty() {
        return;
    }
    let body = TIME_TAG.replace_all(raw, "").trim().to_string();
    let enhanced: Vec<Captures> = ENHANCED_WORD.captures_iter(&body).collect();
    let visible = if !enhanced.is_empty() {
        clean_text(&ENHANCED_WORD.replace_all(&body, "$3"))
    } else {
        clean_text(&body)
    };
    if visible.is_empty() {
        return;
    }
    for tag in &tags {
        let start = timestamp_ms(&tag[1], &tag[2]) + offset;
        if CREDIT_LINE.is_match(&visible) || (start <= 1000 && visible.contains(" - ")) {
            continue;
        }
        let mut chars: Vec<i64> = Vec::new();
        for word in &enhanced {
            let mins = word.get(1).map(|m| m.as_str()).unwrap_or(&tag[1]);
            let word_time = timestamp_ms(mins, &word[2]) + offset;
            let wlen = word[3].chars().count();
            chars.extend(std::iter::repeat(word_time).take(wlen));
        }
        out.push(LyricLine {
            start_ms: start,
            duration_ms: 0,
            text: visible.clone(),
            translation: String::new(),
            romanization: String::new(),
            chars,
            words: Vec::new(),
            song_part: String::new(),
            block_index: 0,
            chorus_flag: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parity fixture: `lyric_sources.py::parse_lrc` output captured on the same string.
    /// Covers offset, meta/credit/title-line skips, multi-timestamp, enhanced-word tags,
    /// and duration inference via finalize.
    #[test]
    fn parse_lrc_matches_python() {
        let lrc = "[ar:Artist]\n\
                   [al:Album]\n\
                   [offset:50]\n\
                   [00:01.00]First line\n\
                   [00:03.00][00:05.00]Double timestamp\n\
                   [00:07.00]<0:1.00>Hello <1.00:1.00>world\n\
                   [00:10.00]作词: someone\n\
                   [00:00.50]Title - Artist\n\
                   [01:00.00]Last line\n";
        let parsed = parse_lrc(lrc);
        // expected: exactly `lyric_sources.py::parse_lrc(lrc)` (Python ground truth)
        let expected = vec![
            (1050i64, 2000i64, "First line".s(), vec![]),
            (3050, 2000, "Double timestamp".s(), vec![]),
            (5050, 2000, "Double timestamp".s(), vec![]),
            (7050, 53000, "Hello <1.00:1.00>world".s(), vec![1050; 6]),
            (60050, 0, "Last line".s(), vec![]),
        ];
        assert_eq!(parsed.len(), expected.len(), "line count mismatch: {parsed:?}");
        for (i, line) in parsed.iter().enumerate() {
            let (start, dur, text, chars) = &expected[i];
            assert_eq!(line.start_ms, *start, "[{i}] start_ms");
            assert_eq!(line.duration_ms, *dur, "[{i}] duration_ms");
            assert_eq!(line.text, *text, "[{i}] text");
            assert_eq!(line.chars, *chars, "[{i}] chars");
            assert_eq!(line.translation, "", "[{i}] translation");
            assert_eq!(line.romanization, "", "[{i}] romanization");
        }
    }

    trait Str { fn s(self) -> &'static str; }
    impl Str for &'static str { fn s(self) -> &'static str { self } }

    #[test]
    fn empty_input_yields_no_lines() {
        assert!(parse_lrc("").is_empty());
        assert!(parse_lrc("[ar:Only Meta]\n[al:Album]\n").is_empty());
    }
}
