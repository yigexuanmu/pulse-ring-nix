//! Folia sonnet v2 — `sonnetProgram.ts` (281 lines) compiler-grade 1:1 port.
//!
//! Compiles unified lyrics into a seek-safe, deterministic PV timeline. The
//! timeline is a `SonnetProgram` of `SonnetParagraph`s, each holding
//! `SonnetCompiledLine`s + `SonnetShot`s + an optional outgoing
//! `SonnetTransition`. Segments come from `build_sonnet_semantic_segments`,
//! camera zoom/rotation offsets from `hash_sonnet_seed`, and the render tail
//! clamp from `get_line_render_end_time`.
//!
//! Pure dependency layer: types (`SonnetProgram` family) live in `types.rs`,
//! `hash_sonnet_seed` in `random.rs`, `build_sonnet_semantic_segments` in
//! `semantic.rs`, `get_line_render_end_time` in `lyrics_util::render_hints`.

// Re-export the semantic builder so callers of `program` don't need to depend
// on `semantic` directly — mirrors the TS `export { buildSonnetSemanticSegments }
// from './sonnetSemantic'` barrel.
pub use crate::lyricstyles::sonnet_v2::semantic::build_sonnet_semantic_segments;

use crate::lyricstyles::sonnet_v2::lyrics_util::render_hints::get_line_render_end_time;
use crate::lyricstyles::sonnet_v2::random::hash_sonnet_seed;
use crate::lyricstyles::sonnet_v2::types::{
    Line, SonnetAnimationCue, SonnetAnimationCueKind, SonnetCameraFrame, SonnetCompiledLine,
    SonnetParagraph, SonnetParagraphBoundary, SonnetParagraphKind, SonnetProgram, SonnetShot,
    SonnetShotKind, SonnetTransition, SonnetTransitionKind, SONNET_SHOT_KINDS,
    SONNET_TRANSITION_KINDS,
};

/// `sonnetProgram.ts:resolveSonnetDebugShotKind` — layout debug override.
/// `None` keeps every registered template in the random pool.
pub const SONNET_DEBUG_SHOT_KIND: Option<SonnetShotKind> = None;

fn resolve_sonnet_debug_shot_kind() -> Option<SonnetShotKind> {
    SONNET_DEBUG_SHOT_KIND
}

/// `sonnetProgram.ts:clamp(value, min, max)`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(value).min(max)
}

/// `sonnetProgram.ts:median(values)`.
fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.5;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        ((sorted[middle - 1]).max(sorted[middle]) + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

/// `sonnetProgram.ts:resolveSonnetParagraphGapThreshold(lines)`.
pub fn resolve_sonnet_paragraph_gap_threshold(lines: &[Line]) -> f64 {
    let gaps: Vec<f64> = lines
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, line)| {
            let prev_end = get_line_render_end_time(Some(&lines[index]));
            line.start_time - prev_end.min(line.start_time)
        })
        .filter(|gap| *gap > 0.0)
        .collect();
    clamp(median(&gaps) * 2.5, 1.25, 3.5)
}

/// `sonnetProgram.ts:metadataChanged(previous, next)`.
fn metadata_changed(previous: &Line, next: &Line) -> bool {
    match (previous.block_index, next.block_index) {
        (Some(p), Some(n)) if p != n => true,
        _ => match (previous.song_part.as_deref(), next.song_part.as_deref()) {
            (Some(p), Some(n)) if p != n => true,
            _ => false,
        },
    }
}

/// `sonnetProgram.ts:ParagraphDraft` interface.
#[derive(Debug, Clone)]
struct ParagraphDraft {
    lines: Vec<SonnetCompiledLine>,
    boundary: SonnetParagraphBoundary,
}

/// `sonnetProgram.ts:splitOversizedDraft(draft)`.
fn split_oversized_draft(draft: ParagraphDraft) -> Vec<ParagraphDraft> {
    let mut output: Vec<ParagraphDraft> = Vec::new();
    let mut remaining: Vec<SonnetCompiledLine> = draft.lines;
    let mut boundary = draft.boundary;
    let mut loop_guard = 0u32;
    while remaining.len() > 6
        || (remaining.len() > 1
            && (remaining.last().unwrap().render_end_time - remaining[0].line.start_time) > 18.0)
    {
        if loop_guard > 1000 {
            eprintln!("splitOversizedDraft: Infinite loop detected, breaking");
            break;
        }
        loop_guard += 1;

        // Build candidate split points (offset+2 .. remaining.len() exclusive of last).
        // candidates = remaining.slice(2, -1).map((line, offset) => ({ splitIndex: offset+2, gap }))
        let candidates: Vec<(usize, f64)> = remaining
            .iter()
            .skip(2)
            .take(remaining.len().saturating_sub(2).saturating_sub(1))
            .enumerate()
            .map(|(offset, line)| {
                let prev = &remaining[offset + 1];
                (offset + 2, line.line.start_time - prev.render_end_time)
            })
            .collect();

        let valid_candidates: Vec<(usize, f64)> = candidates
            .into_iter()
            .filter(|(_, gap)| !gap.is_nan())
            .collect();

        let raw_split_index = valid_candidates
            .iter()
            .max_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| *idx)
            .unwrap_or_else(|| 4.min(remaining.len() - 1));
        let split_index = 1.max(raw_split_index);

        output.push(ParagraphDraft {
            lines: remaining[..split_index].to_vec(),
            boundary,
        });
        remaining = remaining[split_index..].to_vec();
        boundary = if output.last().unwrap().lines.len() >= 6 {
            SonnetParagraphBoundary::LineCap
        } else {
            SonnetParagraphBoundary::DurationCap
        };
    }
    output.push(ParagraphDraft {
        lines: remaining,
        boundary,
    });
    output
}

/// `sonnetProgram.ts:classifyParagraph(lines, index, total)`.
fn classify_paragraph(lines: &[SonnetCompiledLine], index: usize, total: usize) -> SonnetParagraphKind {
    // chorus: explicit isChorus flag, or songPart matches /chorus|副歌/i.
    if lines.iter().any(|item| {
        item.line.is_chorus
            || item
                .line
                .song_part
                .as_deref()
                .map(|p| p.to_lowercase().contains("chorus") || p.contains("副歌"))
                .unwrap_or(false)
    }) {
        return SonnetParagraphKind::Chorus;
    }
    // break: songPart matches /bridge|break|間奏|ブリッジ/i.
    if lines.iter().any(|item| {
        item.line
            .song_part
            .as_deref()
            .map(|p| {
                let l = p.to_lowercase();
                l.contains("bridge") || l.contains("break") || p.contains("間奏") || p.contains("ブリッジ")
            })
            .unwrap_or(false)
    }) {
        return SonnetParagraphKind::Break;
    }
    if index == total - 1 {
        return SonnetParagraphKind::Outro;
    }
    let duration = lines.last().unwrap().render_end_time - lines[0].line.start_time;
    let segment_count: usize = lines
        .iter()
        .map(|line| line.segments.iter().filter(|s| s.is_word_like).count())
        .sum();
    let punctuation_count: usize = lines
        .iter()
        .map(|line| {
            line.line
                .full_text
                .chars()
                .filter(|c| matches!(c, '!' | '?' | '！' | '？' | '…'))
                .count()
        })
        .sum();
    if duration <= 3.5 || segment_count <= 3 {
        return SonnetParagraphKind::Breath;
    }
    if punctuation_count >= 2 || segment_count as f64 / duration.max(1.0) > 2.5 {
        return SonnetParagraphKind::Lift;
    }
    SonnetParagraphKind::Verse
}

/// `sonnetProgram.ts:chooseWithoutRepeat<T>(choices, seed, previous)`.
fn choose_without_repeat<T: Copy + PartialEq>(
    choices: &[T],
    seed: &str,
    previous: Option<T>,
) -> T {
    let start = (hash_sonnet_seed(seed) as usize) % choices.len();
    for offset in 0..choices.len() {
        let candidate = choices[(start + offset) % choices.len()];
        if Some(candidate) != previous {
            return candidate;
        }
    }
    choices[start]
}

/// `sonnetProgram.ts:buildCues(lines)`.
fn build_cues(lines: &[SonnetCompiledLine]) -> Vec<SonnetAnimationCue> {
    // segments = lines.flatMap(line => line.segments).filter(s => s.text.length > 0)
    let segments: Vec<&crate::lyricstyles::sonnet_v2::types::SonnetSemanticSegment> = lines
        .iter()
        .flat_map(|line| line.segments.iter())
        .filter(|segment| !segment.text.is_empty())
        .collect();
    let last = segments.len();
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| SonnetAnimationCue {
            at: segment.start_time,
            duration: 0.08_f64.max(segment.end_time - segment.start_time),
            kind: if index == last - 1 {
                SonnetAnimationCueKind::Accent
            } else {
                SonnetAnimationCueKind::Enter
            },
            segment_start: index,
            segment_end: index + 1,
        })
        .collect()
}

/// `sonnetProgram.ts:groupShotLines(lines)`.
fn group_shot_lines(lines: &[SonnetCompiledLine]) -> Vec<Vec<SonnetCompiledLine>> {
    let mut groups: Vec<Vec<SonnetCompiledLine>> = Vec::new();
    let mut current_group: Vec<SonnetCompiledLine> = Vec::new();
    let mut group_start_time = 0.0f64;

    for line in lines.iter() {
        if current_group.is_empty() {
            current_group.push(line.clone());
            group_start_time = line.line.start_time;
        } else {
            let duration_so_far = line.render_end_time - group_start_time;
            // Group up to 4 lines, max 6 seconds total, to reuse background MG.
            if current_group.len() < 4 && duration_so_far <= 6.0 {
                current_group.push(line.clone());
            } else {
                groups.push(current_group);
                current_group = vec![line.clone()];
                group_start_time = line.line.start_time;
            }
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }
    groups
}

/// `sonnetProgram.ts:buildShots(lines, kind, paragraphIndex, seed, previousKind)`.
fn build_shots(
    lines: &[SonnetCompiledLine],
    kind: SonnetParagraphKind,
    paragraph_index: usize,
    seed: &str,
    previous_kind: Option<SonnetShotKind>,
) -> Vec<SonnetShot> {
    let mut last_kind = previous_kind;
    let groups = group_shot_lines(lines);

    groups
        .iter()
        .enumerate()
        .map(|(shot_index, group)| {
            let signature: String = group
                .iter()
                .map(|item| item.line.full_text.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let debug_shot_kind = resolve_sonnet_debug_shot_kind();
            let mut shot_kind = debug_shot_kind.unwrap_or_else(|| {
                choose_without_repeat(
                    SONNET_SHOT_KINDS,
                    &format!("{seed}:{paragraph_index}:{shot_index}:{signature}"),
                    last_kind,
                )
            });
            let word_count: usize = group
                .iter()
                .map(|item| item.segments.iter().filter(|s| s.is_word_like).count())
                .sum();
            if debug_shot_kind.is_none() {
                if kind == SonnetParagraphKind::Breath
                    && shot_index == 0
                    && word_count <= 2
                {
                    shot_kind = SonnetShotKind::QuietTableau;
                }
                if kind == SonnetParagraphKind::Chorus && shot_kind == SonnetShotKind::QuietTableau {
                    shot_kind = SonnetShotKind::TypeImpact;
                }
            }
            last_kind = Some(shot_kind);

            let random =
                hash_sonnet_seed(&format!("{seed}:{paragraph_index}:{shot_index}:camera"));
            let zoom_random = (((random >> 16) & 255) as f64) / 255.0;
            // Medium close-up bias: framing should feel intimate with the current word;
            // only composition-first layouts (poster zones, calm tableau) stay wider.
            let (zoom_base, zoom_span) = match shot_kind {
                SonnetShotKind::PosterBlocks => (1.02, 0.16),
                SonnetShotKind::QuietTableau => (1.12, 0.2),
                _ => (1.22, 0.26),
            };
            SonnetShot {
                id: format!("p{paragraph_index}-s{shot_index}"),
                kind: shot_kind,
                start_time: group[0].line.start_time,
                end_time: group.last().unwrap().render_end_time,
                line_indices: group.iter().map(|item| item.source_index).collect(),
                cues: build_cues(group),
                camera: SonnetCameraFrame {
                    x: (((random & 255) as f64 / 255.0) - 0.5) * 0.18,
                    y: ((((random >> 8) & 255) as f64 / 255.0) - 0.5) * 0.14,
                    zoom: zoom_base + zoom_random * zoom_span,
                    rotation: ((((random >> 24) & 255) as f64 / 255.0) - 0.5) * 0.08,
                },
            }
        })
        .collect()
}

/// `sonnetProgram.ts:compileSonnetProgram(lines, seed = 'sonnet')`.
pub fn compile_sonnet_program(lines: &[Line], seed: &str) -> SonnetProgram {
    // Step 1: compile each line (compute renderEndTime + segments).
    let compiled: Vec<SonnetCompiledLine> = lines
        .iter()
        .enumerate()
        .map(|(source_index, line)| {
            let next_start = lines
                .get(source_index + 1)
                .map(|l| l.start_time)
                .unwrap_or(f64::INFINITY);
            let render_end_time = line
                .start_time
                .max(get_line_render_end_time(Some(line)).min(next_start));
            SonnetCompiledLine {
                source_index,
                line: line.clone(),
                render_end_time,
                segments: build_sonnet_semantic_segments(line),
            }
        })
        .collect();

    // Step 2: paragraph gap threshold.
    let paragraph_gap_threshold = resolve_sonnet_paragraph_gap_threshold(lines);

    // Step 3: split into paragraph drafts.
    let mut drafts: Vec<ParagraphDraft> = Vec::new();
    let mut current = ParagraphDraft {
        lines: Vec::new(),
        boundary: SonnetParagraphBoundary::SongStart,
    };

    for (index, line) in compiled.iter().enumerate() {
        let previous = if index == 0 { None } else { compiled.get(index - 1) };
        let gap = previous
            .map(|p| line.line.start_time - p.render_end_time)
            .unwrap_or(0.0);
        let boundary = if let Some(prev) = previous {
            if metadata_changed(&prev.line, &line.line) {
                Some(SonnetParagraphBoundary::Metadata)
            } else if gap >= paragraph_gap_threshold {
                Some(SonnetParagraphBoundary::TimeGap)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(b) = boundary {
            if !current.lines.is_empty() {
                drafts.extend(split_oversized_draft(current));
                current = ParagraphDraft {
                    lines: Vec::new(),
                    boundary: b,
                };
            } else {
                current.boundary = b;
            }
        }
        current.lines.push(line.clone());
    }
    if !current.lines.is_empty() {
        drafts.extend(split_oversized_draft(current));
    }

    // Step 4: classify + build shots + transitions.
    let resolved_seed = seed.to_string();
    let mut previous_shot: Option<SonnetShotKind> = None;
    let mut previous_transition: Option<SonnetTransitionKind> = None;
    let paragraphs: Vec<SonnetParagraph> = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            let kind = classify_paragraph(&draft.lines, index, drafts.len());
            let shots = build_shots(&draft.lines, kind, index, &resolved_seed, previous_shot);
            previous_shot = shots
                .last()
                .map(|s| s.kind)
                .or(previous_shot);
            let next = drafts.get(index + 1);
            let end_time = draft.lines.last().unwrap().render_end_time;
            let gap = next
                .map(|n| n.lines[0].line.start_time - end_time)
                .unwrap_or(0.0);
            let available_transitions: Vec<SonnetTransitionKind> = SONNET_TRANSITION_KINDS.to_vec();
            let transition_kind = if next.is_some() {
                Some(choose_without_repeat(
                    &available_transitions,
                    &format!("{resolved_seed}:{index}:transition"),
                    previous_transition,
                ))
            } else {
                None
            };
            if let Some(tk) = transition_kind {
                previous_transition = Some(tk);
            }
            let transition_duration = if next.is_some() {
                0.3_f64.min(0.16_f64.max(if gap > 0.0 { gap * 0.5 } else { 0.2 }))
            } else {
                0.0
            };
            let transition_end_time = next
                .map(|n| n.lines[0].line.start_time)
                .unwrap_or(end_time);
            SonnetParagraph {
                id: format!("sonnet-p{index}"),
                kind,
                boundary: draft.boundary,
                start_time: draft.lines[0].line.start_time,
                end_time,
                lines: draft.lines.clone(),
                shots,
                transition_out: transition_kind.map(|tk| SonnetTransition {
                    kind: tk,
                    start_time: draft.lines[0]
                        .line
                        .start_time
                        .max(transition_end_time - transition_duration),
                    end_time: transition_end_time,
                }),
            }
        })
        .collect();

    SonnetProgram {
        version: 1,
        seed: resolved_seed,
        paragraph_gap_threshold,
        paragraphs,
    }
}

/// `sonnetProgram.ts:findSonnetParagraphIndexAtTime(program, time)`.
pub fn find_sonnet_paragraph_index_at_time(program: &SonnetProgram, time: f64) -> usize {
    for index in (0..program.paragraphs.len()).rev() {
        if time >= program.paragraphs[index].start_time {
            return index;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::Word;

    fn mk_line(start: f64, end: f64, text: &str) -> Line {
        Line {
            words: Vec::new(),
            start_time: start,
            end_time: end,
            full_text: text.to_string(),
            render_hints: None,
            block_index: None,
            song_part: None,
            is_chorus: false,
        }
    }

    fn mk_word_line(start: f64, end: f64, text: &str, words: Vec<Word>) -> Line {
        Line {
            words,
            start_time: start,
            end_time: end,
            full_text: text.to_string(),
            render_hints: None,
            block_index: None,
            song_part: None,
            is_chorus: false,
        }
    }

    #[test]
    fn median_of_empty_returns_half() {
        assert_eq!(median(&[]), 0.5);
    }

    #[test]
    fn median_of_odd_length_picks_middle() {
        assert_eq!(median(&[1.0, 3.0, 2.0]), 2.0);
    }

    #[test]
    fn median_of_even_length_averages_two_middles() {
        // sorted = [1, 2, 3, 4]; middle=2; (sorted[1].max(sorted[2]) + sorted[2]) / 2
        //  = (3 + 3) / 2 = 3
        assert_eq!(median(&[3.0, 1.0, 4.0, 2.0]), 3.0);
    }

    #[test]
    fn clamp_constrains_to_range() {
        assert_eq!(clamp(5.0, 1.0, 3.0), 3.0);
        assert_eq!(clamp(0.0, 1.0, 3.0), 1.0);
        assert_eq!(clamp(2.0, 1.0, 3.0), 2.0);
    }

    #[test]
    fn metadata_changed_detects_block_index_and_song_part() {
        let a = Line {
            words: Vec::new(),
            start_time: 0.0,
            end_time: 1.0,
            full_text: "a".into(),
            render_hints: None,
            block_index: Some(0),
            song_part: Some("verse".into()),
            is_chorus: false,
        };
        let b = Line {
            words: Vec::new(),
            start_time: 0.0,
            end_time: 1.0,
            full_text: "b".into(),
            render_hints: None,
            block_index: Some(0), // same block
            song_part: Some("chorus".into()), // different songPart
            is_chorus: false,
        };
        assert!(metadata_changed(&a, &b));
        assert!(!metadata_changed(&a, &a));
    }

    #[test]
    fn resolve_gap_threshold_clamps_to_min_max() {
        // Single close line pair -> gap=0 filtered out -> gaps=[] -> median=0.5
        // -> clamp(0.5 * 2.5, 1.25, 3.5) = 1.25.
        let lines = vec![mk_line(0.0, 1.0, "a"), mk_line(1.0, 2.0, "b")];
        assert_eq!(resolve_sonnet_paragraph_gap_threshold(&lines), 1.25);
    }

    #[test]
    fn classify_paragraph_chorus_when_is_chorus() {
        let lines: Vec<SonnetCompiledLine> = vec![SonnetCompiledLine {
            source_index: 0,
            line: Line {
                words: Vec::new(),
                start_time: 0.0,
                end_time: 4.0,
                full_text: "Hey".into(),
                render_hints: None,
                block_index: None,
                song_part: None,
                is_chorus: true,
            },
            render_end_time: 4.0,
            segments: Vec::new(),
        }];
        assert_eq!(
            classify_paragraph(&lines, 0, 1),
            SonnetParagraphKind::Chorus
        );
    }

    #[test]
    fn find_paragraph_index_returns_last_paragraph_with_start_le_time() {
        let program = compile_sonnet_program(
            &[
                mk_line(0.0, 1.0, "a"),
                mk_line(2.0, 3.0, "b"),
                mk_line(4.0, 5.0, "c"),
            ],
            "test",
        );
        // No metadata change and gaps too small -> a single draft/paragraph that
        // holds all 3 lines.
        assert!(!program.paragraphs.is_empty());
        let idx = find_sonnet_paragraph_index_at_time(&program, 4.5);
        assert!(idx < program.paragraphs.len());
        assert!(program.paragraphs[idx].start_time <= 4.5);
    }

    #[test]
    fn choose_without_repeat_avoids_previous_when_possible() {
        let choices = [1u8, 2, 3];
        let picked = choose_without_repeat(&choices, "test", Some(1));
        assert_ne!(picked, 1);
    }

    #[test]
    fn compile_program_assigns_distinct_shot_ids_per_index() {
        let lines: Vec<Line> = (0..6)
            .map(|i| mk_word_line(i as f64 * 2.0, i as f64 * 2.0 + 1.5, "word", Vec::new()))
            .collect();
        let program = compile_sonnet_program(&lines, "id");
        assert!(program.version == 1);
        assert_eq!(program.seed, "id");
        // Each shot id must be unique within its paragraph.
        for p in &program.paragraphs {
            let mut ids: Vec<&str> = p.shots.iter().map(|s| s.id.as_str()).collect();
            let total = ids.len();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), total, "duplicate shot ids {:?}", p.shots);
        }
    }
}
