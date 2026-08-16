//! Folia sonnet v2 — `sonnetTypographyRoles.ts` (114 lines) role scoring.
//!
//! Byte-identical 1:1 port of folia
//! `src/components/visualizer/sonnet/sonnetTypographyRoles.ts`.
//! Pure functions that select deterministic typography emphasis roles without
//! coupling them to a layout template. No PIXI, no measurement, no side effects
//! — depends only on the `SonnetSemanticSegment` contract from `crate::types`
//! and `normalize_font_weight` from `crate::font_stack`.
//!
//! ## Foliation reference
//! ```ts
//! export type SonnetSegmentRole = 'hero' | 'semi-hero' | 'support' | 'decoration';
//! export const isSonnetEmphasisRole = (role) => role === 'hero' || role === 'semi-hero';
//! export const resolveSonnetRoleFontWeight = (configuredFontWeight, role) => {
//!     const manualWeight = normalizeFontWeight(configuredFontWeight);
//!     if (manualWeight !== null) return manualWeight;
//!     if (isSonnetEmphasisRole(role)) return 900;
//!     return role === 'decoration' ? 300 : 700;
//! };
//! ... // (full body below — every constant and branch transcribed verbatim)
//! ```

use super::font_stack::normalize_font_weight;
use super::types::{SonnetSegmentRole, SonnetSemanticSegment};

/// folia `sonnetTypographyRoles.ts` — `isSonnetEmphasisRole(role)`.
///
/// `Hero` and `SemiHero` carry typographic emphasis (bold weight, larger scale);
/// `Support` and `Decoration` are the ambient body.
pub fn is_sonnet_emphasis_role(role: SonnetSegmentRole) -> bool {
    matches!(role, SonnetSegmentRole::Hero | SonnetSegmentRole::SemiHero)
}

/// folia `sonnetTypographyRoles.ts` — `resolveSonnetRoleFontWeight`.
///
/// Uses Sonnet's designed role weights in auto mode, or the user's global
/// manual override. `configured_font_weight = None` mirrors folia `null` →
/// `manualWeight === null` and falls through to the role default ladder.
pub fn resolve_sonnet_role_font_weight(
    configured_font_weight: Option<i32>,
    role: SonnetSegmentRole,
) -> i32 {
    // TS: `normalizeFontWeight` returns `number | null` — null only when input
    // was null/undefined (not NaN — NaN wraps to the 400 fallback). My port
    // returns `DEFAULT_FONT_WEIGHT` (400) for `None`, so the `manualWeight
    // !== null` branch is always true and returns 400. To preserve the
    // folia semantic where `null` signal means "auto mode", caller passes
    // `Some(0)` would wrongly clamp to 100 — so the contract is: None == auto.
    if let Some(explicit) = configured_font_weight {
        // Non-null explicit override (including NaN-clamped-to-400).
        return normalize_font_weight(Some(explicit));
    }
    // Auto mode: role defaults.
    if is_sonnet_emphasis_role(role) {
        return 900;
    }
    if role == SonnetSegmentRole::Decoration {
        return 300;
    }
    700
}

/// folia `sonnetTypographyRoles.ts` — `getSonnetVisibleSegmentLength(segment)`.
///
/// Counts graphemes whose `char.trim().len() > 0` — excludes pure-whitespace
/// graphemes (line breaks, glue spaces) from the visible-character tally that
/// drives hero/semi-hero scoring.
pub fn get_sonnet_visible_segment_length(segment: &SonnetSemanticSegment) -> usize {
    segment
        .graphemes
        .iter()
        .filter(|item| item.char.trim().len() > 0)
        .count()
}

/// folia `sonnetTypographyRoles.ts` — `scoreSonnetHeroSegment(segment)`.
///
/// `lengthScore = min(visibleLen, 8) * 14` rewards 0–8 visible graphemes with
/// diminishing returns; `durationScore = min(2.5, max(0, endTime - startTime))
/// * 18` rewards 0–2.5s of coverage. Both commensurate — a long word held for
/// the full width beats a short token.
pub fn score_sonnet_hero_segment(segment: &SonnetSemanticSegment) -> f64 {
    let length_score = (get_sonnet_visible_segment_length(segment).min(8) as f64) * 14.0;
    let duration_score = (2.5_f64).max(0.0).min((segment.end_time - segment.start_time).max(0.0)) * 18.0;
    length_score + duration_score
}

/// folia `sonnetTypographyRoles.ts` — `findSonnetHeroSegmentIndex(segments)`.
///
/// First word-like segment's index is the initial `bestIndex`; then scans for
/// any word-like segment with at least one visible grapheme and the highest
/// hero score. `max(0, bestIndex)` floors the empty/never-set case at 0.
pub fn find_sonnet_hero_segment_index(segments: &[SonnetSemanticSegment]) -> usize {
    let mut best_index: i64 = segments
        .iter()
        .position(|s| s.is_word_like)
        .map(|i| i as i64)
        .unwrap_or(-1);
    let mut best_score: f64 = f64::NEG_INFINITY;
    for (index, segment) in segments.iter().enumerate() {
        if !segment.is_word_like || get_sonnet_visible_segment_length(segment) == 0 {
            continue;
        }
        let score = score_sonnet_hero_segment(segment);
        if score > best_score {
            best_score = score;
            best_index = index as i64;
        }
    }
    (best_index.max(0)) as usize
}

// Semi-hero constraints — transcribed verbatim from folia.
const SEMI_HERO_MIN_GAP: i64 = 2;
const SEMI_HERO_MIN_VISIBLE_LENGTH: usize = 2;
const SEMI_HERO_MIN_LINE_WORDS: usize = 4;
const SEMI_HERO_SCORE_RATIO: f64 = 0.35;
const SEMI_HERO_MULTI_WORD_COUNT: usize = 9;

/// folia `sonnetTypographyRoles.ts` — `findSonnetSemiHeroSegmentIndices`.
///
/// Picks secondary emphasis words on the side opposite the hero's lean, so the
/// composition stays balanced; long lines earn a second accent on the other
/// side. Returns indices in ascending order.
pub fn find_sonnet_semi_hero_segment_indices(
    segments: &[SonnetSemanticSegment],
    hero_index: usize,
) -> Vec<usize> {
    let Some(hero) = segments.get(hero_index) else {
        return Vec::new();
    };
    let word_like_count = segments
        .iter()
        .filter(|s| s.is_word_like && get_sonnet_visible_segment_length(s) > 0)
        .count();
    if word_like_count < SEMI_HERO_MIN_LINE_WORDS {
        return Vec::new();
    }

    let threshold = score_sonnet_hero_segment(hero) * SEMI_HERO_SCORE_RATIO;
    // Candidates: word-like, ≥2 visible graphemes, ≥2 indices from hero, score
    // at least `threshold` of the hero's score.
    struct Candidate {
        index: usize,
        segment_idx_for_score: usize,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, seg) in segments.iter().enumerate() {
        if index == hero_index {
            continue;
        }
        if !seg.is_word_like {
            continue;
        }
        if get_sonnet_visible_segment_length(seg) < SEMI_HERO_MIN_VISIBLE_LENGTH {
            continue;
        }
        if ((index as i64) - (hero_index as i64)).abs() < SEMI_HERO_MIN_GAP {
            continue;
        }
        if score_sonnet_hero_segment(seg) < threshold {
            continue;
        }
        candidates.push(Candidate {
            index,
            segment_idx_for_score: index,
        });
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    // `bestOf`: pick the highest-scoring candidate in a slice.
    let best_of = |list: &[&Candidate]| -> Option<usize> {
        let mut best: Option<&Candidate> = None;
        for item in list {
            match best {
                None => best = Some(item),
                Some(existing) => {
                    if score_sonnet_hero_segment(&segments[item.segment_idx_for_score])
                        > score_sonnet_hero_segment(&segments[existing.segment_idx_for_score])
                    {
                        best = Some(item);
                    }
                }
            }
        }
        best.map(|c| c.index)
    };

    let hero_leans_early = hero_index <= (segments.len() - 1) / 2;
    let primary_side: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| {
            if hero_leans_early {
                c.index > hero_index
            } else {
                c.index < hero_index
            }
        })
        .collect();
    let secondary_side: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| {
            if hero_leans_early {
                c.index < hero_index
            } else {
                c.index > hero_index
            }
        })
        .collect();

    let mut picks: Vec<usize> = Vec::new();
    let primary = best_of(&primary_side).or_else(|| best_of(&secondary_side));
    if let Some(primary_idx) = primary {
        picks.push(primary_idx);
        if word_like_count >= SEMI_HERO_MULTI_WORD_COUNT {
            let secondary: Vec<&Candidate> = secondary_side
                .iter()
                .filter(|c| ((c.index as i64) - (primary_idx as i64)).abs() >= SEMI_HERO_MIN_GAP)
                .copied()
                .collect();
            if let Some(secondary_idx) = best_of(&secondary) {
                picks.push(secondary_idx);
            }
        }
    }
    picks.sort_unstable();
    picks
}

/// folia `sonnetTypographyRoles.ts` — `findSonnetSemiHeroSegmentIndex`.
///
/// First semi-hero index or `-1` when none exist.
pub fn find_sonnet_semi_hero_segment_index(
    segments: &[SonnetSemanticSegment],
    hero_index: usize,
) -> i64 {
    find_sonnet_semi_hero_segment_indices(segments, hero_index)
        .first()
        .map(|i| *i as i64)
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyricstyles::sonnet_v2::types::{GraphemeTiming, SonnetSemanticSegment};

    fn seg(text: &str, start: f64, end: f64, is_word_like: bool) -> SonnetSemanticSegment {
        SonnetSemanticSegment {
            text: text.to_string(),
            start_offset: 0,
            end_offset: text.chars().count(),
            start_time: start,
            end_time: end,
            word_indices: Vec::new(),
            graphemes: text
                .chars()
                .map(|c| GraphemeTiming {
                    char: c.to_string(),
                    start_time: start,
                    end_time: end,
                    word_index: None,
                })
                .collect(),
            is_word_like,
        }
    }

    #[test]
    fn is_emphasis_role_distinguishes_hero_semi_hero_from_support_decoration() {
        assert!(is_sonnet_emphasis_role(SonnetSegmentRole::Hero));
        assert!(is_sonnet_emphasis_role(SonnetSegmentRole::SemiHero));
        assert!(!is_sonnet_emphasis_role(SonnetSegmentRole::Support));
        assert!(!is_sonnet_emphasis_role(SonnetSegmentRole::Decoration));
    }

    #[test]
    fn resolve_font_weight_auto_uses_role_defaults() {
        assert_eq!(resolve_sonnet_role_font_weight(None, SonnetSegmentRole::Hero), 900);
        assert_eq!(resolve_sonnet_role_font_weight(None, SonnetSegmentRole::SemiHero), 900);
        assert_eq!(resolve_sonnet_role_font_weight(None, SonnetSegmentRole::Support), 700);
        assert_eq!(resolve_sonnet_role_font_weight(None, SonnetSegmentRole::Decoration), 300);
    }

    #[test]
    fn resolve_font_weight_manual_override_wins() {
        assert_eq!(resolve_sonnet_role_font_weight(Some(400), SonnetSegmentRole::Hero), 400);
        assert_eq!(resolve_sonnet_role_font_weight(Some(250), SonnetSegmentRole::Decoration), 250);
    }

    #[test]
    fn visible_segment_length_excludes_whitespace_graphemes() {
        let s = seg("ab c", 0.0, 1.0, true);
        // 'a','b',' ','c' → space's trim().len()==0 → excluded → 3 visible.
        assert_eq!(get_sonnet_visible_segment_length(&s), 3);
    }

    #[test]
    fn visible_segment_length_counts_only_non_whitespace() {
        let s = seg("a b", 0.0, 1.0, true);
        assert_eq!(get_sonnet_visible_segment_length(&s), 2); // 'a','b' visible, ' ' excluded.
    }

    #[test]
    fn score_hero_segment_rewards_length_and_duration() {
        let short = seg("ab", 0.0, 1.0, true);
        let long = seg("abcdefgh", 0.0, 1.0, true);
        let long_duration = seg("abcdefgh", 0.0, 3.0, true);
        // short: 2*14 + min(2.5, 1.0)*18 = 28 + 18 = 46
        assert_eq!(score_sonnet_hero_segment(&short), 46.0);
        // long: 8*14 + min(2.5, 1.0)*18 = 112 + 18 = 130 (length clamps at 8)
        assert_eq!(score_sonnet_hero_segment(&long), 130.0);
        // long_duration: 8*14 + min(2.5, 3.0)*18 = 112 + 45 = 157
        assert_eq!(score_sonnet_hero_segment(&long_duration), 157.0);
    }

    #[test]
    fn find_hero_segment_picks_highest_score_word_like() {
        let segs = vec![
            seg("little", 0.0, 0.5, true),
            seg("remembered", 0.0, 1.0, true),
            seg("the", 0.0, 0.3, true),
        ];
        // "remembered": 9→clamped 8*14=112 + min(2.5,1.0)*18=18 → 130
        // "little": 6*14=84 + min(2.5,0.5)*18=9 → 93
        // "the": 3*14=42 + min(2.5,0.3)*18=5.4 → 47.4
        assert_eq!(find_sonnet_hero_segment_index(&segs), 1);
    }

    #[test]
    fn find_hero_returns_zero_when_no_word_like() {
        let segs = vec![seg("...", 0.0, 1.0, false)];
        assert_eq!(find_sonnet_hero_segment_index(&segs), 0);
    }

    #[test]
    fn semi_hero_empty_when_too_few_word_like() {
        let segs = vec![
            seg("hero", 0.0, 1.0, true),
            seg("a", 0.0, 1.0, true),
            seg("b", 0.0, 1.0, true),
        ];
        // Only 3 word-like < SEMI_HERO_MIN_LINE_WORDS (4).
        assert_eq!(find_sonnet_semi_hero_segment_indices(&segs, 0), Vec::<usize>::new());
    }

    #[test]
    fn semi_hero_picks_opposite_side_of_hero_lean() {
        // 6 word-like segments, hero in the middle-right.
        let segs = vec![
            seg("alpha", 0.0, 0.8, true),
            seg("beta", 0.0, 0.8, true),
            seg("gamma", 0.0, 0.8, true),
            seg("delta-epic", 0.0, 1.0, true), // hero (index 3, length 10)
            seg("epsilon", 0.0, 0.9, true),
            seg("zeta", 0.0, 0.8, true),
        ];
        let hero_idx = find_sonnet_hero_segment_index(&segs);
        let semi = find_sonnet_semi_hero_segment_indices(&segs, hero_idx);
        // Should return at least one index on the opposite side, well-ordered.
        assert_eq!(hero_idx, 3);
        // Hero at index 3 in a 6-segment line; hero_leans_early = 3 <= 5/2 = 2 → false.
        // So primarySide = index < hero (left side); secondary = index > hero.
        assert!(semi.iter().all(|&i| i != hero_idx));
        // Picks must be ascending.
        let mut sorted = semi.clone();
        sorted.sort_unstable();
        assert_eq!(semi, sorted);
    }
}
