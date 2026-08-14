//! Sonnet style ("商籁") — a faithful port of folia's cinematic lyric engine: semantic roles,
//! per-word kinetic fly-in, seven typographic templates with measured no-overlap layouts, a
//! shot camera (path easing + handheld breath + focus tracking + shake), fast-blur/glitch
//! transitions, post-processing (grain/contrast), decorative open frames and the full
//! motion-graphics decorative background (HUD, geometric chaos, fixed geometry, particles,
//! scanlines — all rendered procedurally from vector commands, no images).

use crate::lyricview::{
    CharQuad, FontScales, LineTiming, LyricFx, StyleCtx, StyleInput, StyleOutput, apply_camera_local,
    measure_text, measure_text_bold, push_rect, push_word_full, split_with_timing,
};
use crate::sdf::{CELL, PAD, RASTER_PX};

// Cache the per-shot MG decoration scene (its vector commands are expensive to rebuild; the
// actual per-frame emission is cheap). Keyed by seed + shot index, bounded to the last two.
thread_local! {
    static MG_CACHE: std::cell::RefCell<std::collections::HashMap<(u64, usize), crate::lyricstyles::mg_scene::MgScene>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

// Cache the per-shot lyric placements. Without this, build_placements is called every frame
// and any per-frame non-determinism in the layout would make the "position shifts when the
// next character comes" bug visible. With the cache, positions are locked to the shot.
thread_local! {
    static PLACEMENT_CACHE: std::cell::RefCell<std::collections::HashMap<(u64, usize), Vec<Placement>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Cached result of `compile_program` keyed by `(lines_ptr, lines_len, seed)`.
/// The program only changes when the lyric lines themselves change, so re-running
/// `compile_program` every frame was pure waste (sorts gaps, classifies paragraphs,
/// picks shot kinds). Keyed by raw pointer + length so it's free to compute but
/// invalidates automatically when the worker thread hands us a new `LyricData`.
thread_local! {
    static PROGRAM_CACHE: std::cell::RefCell<
        std::collections::HashMap<(*const crate::lyrics::LyricLine, usize, u64), Program>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

// ---------------------------------------------------------------- roles

#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Hero,
    SemiHero,
    Support,
    Decoration,
}

// ---------------------------------------------------------------- easing

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn smooth(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

/// CSS-style cubic-bezier timing: solve the x curve by bisection, then sample y (folia).
fn resolve_cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, value: f32) -> f32 {
    let target = clamp01(value);
    if target == 0.0 || target == 1.0 {
        return target;
    }
    let cubic_x = |t: f32| {
        let it = 1.0 - t;
        3.0 * it * it * t * x1 + 3.0 * it * t * t * x2 + t * t * t
    };
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    let mut param = target;
    for _ in 0..12 {
        let x = cubic_x(param);
        if x < target {
            lo = param;
        } else {
            hi = param;
        }
        param = (lo + hi) * 0.5;
    }
    let t = param;
    let it = 1.0 - t;
    3.0 * it * it * t * y1 + 3.0 * it * t * t * y2 + t * t * t
}

fn ease_in_out(v: f32) -> f32 {
    resolve_cubic_bezier(0.65, 0.0, 0.35, 1.0, v)
}

fn ease_enter(v: f32) -> f32 {
    resolve_cubic_bezier(0.22, 1.0, 0.36, 1.0, v)
}

fn ease_expo_out(v: f32) -> f32 {
    let t = clamp01(v);
    if t >= 1.0 { 1.0 } else { 1.0 - 2.0f32.powf(-10.0 * t) }
}

fn ease_elastic_out(v: f32) -> f32 {
    let t = clamp01(v);
    const P: f32 = 0.35;
    2.0f32.powf(-10.0 * t) * ((t - P / 4.0) * (2.0 * std::f32::consts::PI) / P).sin() + 1.0
}

// ---------------------------------------------------------------- program

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShotKind {
    EditorialColumn,
    TypeImpact,
    FragmentCollage,
    TrackingRibbon,
    MaskReveal,
    PosterBlocks,
    QuietTableau,
}

const SHOT_KINDS: [ShotKind; 7] = [
    ShotKind::EditorialColumn,
    ShotKind::TypeImpact,
    ShotKind::FragmentCollage,
    ShotKind::TrackingRibbon,
    ShotKind::MaskReveal,
    ShotKind::PosterBlocks,
    ShotKind::QuietTableau,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionKind {
    FastBlur,
    MonoGlitch,
    CameraPull,
}

#[derive(Debug, Clone, Copy)]
struct Camera {
    tx: f32,
    ty: f32,
    zoom: f32,
    rot: f32,
}

#[derive(Debug, Clone)]
struct Shot {
    start: f32,
    end: f32,
    kind: ShotKind,
    variant: u32,
    transition: TransitionKind,
    /// Shot-boundary transition window (folia min(0.24, max(0.14, gap*0.18))).
    twindow: f32,
    line_range: std::ops::Range<usize>,
    camera: Camera,
}

#[derive(Debug, Clone)]
struct Program {
    shots: Vec<Shot>,
    /// Paragraph boundaries (start, end, transition kind, window) for paragraph transitions.
    paras: Vec<(f32, f32, TransitionKind, f32)>,
}

// folia sonnetRandom — deterministic, stateless 32-bit FNV-1a + golden hash.
// Every random selection in folia is *stateless*: each draw is
// `hashSonnetSeed("{seed}:{para}:{shot}:{tag}")`, so the same lyric line at the same
// passage always picks the same shot kind / variant / camera, with no PRNG state
// drifting across the song. The previous 64-bit xorshift Seeded was stateful and
// therefore drifted, mismatching folia on rebuild.
const FNV_OFFSET: u32 = 2166136261;
const FNV_PRIME: u32 = 16777619;
const SONNET_GOLDEN: u32 = 2654435761;

/// FNV-1a hash of a UTF-8 string (folia `hashSonnetSeed`).
fn hash_sonnet_seed(s: &str) -> u32 {
    let mut h = FNV_OFFSET;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}
/// Golden mix of `(seed ^ salt)` (folia `mixSonnetSeed`).
fn mix_sonnet_seed(seed: u32, salt: u32) -> u32 {
    (seed ^ salt).wrapping_mul(SONNET_GOLDEN)
}
/// Deterministic 0..1 jitter per `(seed, index, salt)` (folia `sonnetHash01`).
fn sonnet_hash01(seed: u32, index: u32, salt: u32) -> f32 {
    let s = seed.wrapping_add((index.wrapping_add(1)).wrapping_mul(97));
    mix_sonnet_seed(s, salt) as f32 / 4294967296.0
}

// Local PRNG for *layout* jitter (fragment spread, poster rotation, MG decor
// placement). folia uses `Math.random()` / `sonnetHash01` for these (stateless per
// item), but the port chains a 32-bit golden mix drawn from the shot seed so the
// layout is deterministic and rebuild-stable within a shot. Layout jitter only
// affects *spatial spread* — never the program-level shot/camera picks that drive
// "same lyric → same shot" — so a stateful chain here is harmless for fidelity.
struct Seeded {
    state: u32,
}
impl Seeded {
    fn new(seed: u64) -> Self {
        Self { state: mix_sonnet_seed(seed as u32, 0x5EED) }
    }
    fn next(&mut self) -> u32 {
        self.state = mix_sonnet_seed(self.state, 0xBEEF);
        self.state
    }
    fn unit(&mut self) -> f32 {
        self.next() as f32 / 4294967296.0
    }
}

fn line_end(line: &crate::lyrics::LyricLine, next: Option<&crate::lyrics::LyricLine>) -> f32 {
    let start = line.start_ms as f32 / 1000.0;
    let mut end = start + line.duration_ms as f32 / 1000.0;
    if line.duration_ms <= 0 {
        if let Some(n) = next {
            end = n.start_ms as f32 / 1000.0;
        }
    }
    if let Some(n) = next {
        end = end.min(n.start_ms as f32 / 1000.0);
    }
    end.max(start + 0.1)
}

fn compile_program(lines: &[crate::lyrics::LyricLine], seed: u64) -> Program {
    if lines.is_empty() {
        return Program { shots: vec![], paras: vec![] };
    }
    let mut gaps: Vec<f32> = Vec::new();
    for i in 0..lines.len().saturating_sub(1) {
        let gap = (lines[i + 1].start_ms as f32 / 1000.0) - line_end(&lines[i], Some(&lines[i + 1]));
        if gap > 0.0 {
            gaps.push(gap);
        }
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = gaps.get(gaps.len() / 2).copied().unwrap_or(1.0);
    let gap_threshold = (median * 2.5).clamp(1.25, 3.5);

    let mut para_bounds: Vec<(usize, usize)> = Vec::new();
    let mut para_start = 0usize;
    for i in 1..lines.len() {
        let gap = (lines[i].start_ms as f32 / 1000.0) - line_end(&lines[i - 1], Some(&lines[i]));
        if gap >= gap_threshold || i - para_start >= 10 {
            para_bounds.push((para_start, i));
            para_start = i;
        }
    }
    para_bounds.push((para_start, lines.len()));

    let mut shots: Vec<Shot> = Vec::new();
    let mut prev_kind: Option<ShotKind> = None;
    let mut prev_transition: Option<TransitionKind> = None;
    for (para_idx, &(ps, pe)) in para_bounds.iter().enumerate() {
        // Paragraph kind (folia classifyParagraph): chorus / breath / verse heuristics.
        let para_dur = line_end(&lines[pe - 1], None) - lines[ps].start_ms as f32 / 1000.0;
        let para_words: usize = lines[ps..pe].iter().map(|l| split_with_timing(l, &LineTiming { start: l.start_ms as f32 / 1000.0, end: line_end(l, None), duration: 1.0 }).len()).sum();
        let text_all: String = lines[ps..pe].iter().map(|l| l.text.clone()).collect::<Vec<_>>().join(" ");
        let low = text_all.to_lowercase();
        // folia classifyParagraph: chorus / break / breath / lift / verse.
        let is_chorus = low.contains("chorus") || text_all.contains("副歌");
        let is_break = low.contains("bridge") || low.contains("break") || text_all.contains("间奏") || text_all.contains("桥");
        let is_breath = !is_chorus && !is_break && (para_dur <= 3.5 || para_words <= 3);
        let is_lift = !is_chorus && !is_break && (text_all.matches(['!', '?', '！', '？', '…']).count() >= 2 || para_words as f32 / para_dur.max(1.0) > 2.5);
        // folia classifyParagraph: the final paragraph is always 'outro' (no duration gate).
        let is_outro = pe >= lines.len();
        let _ = (is_lift, is_outro);
        // Shot grouping thresholds. Previously (group.len() < 4 && dur <= 6s && para <= 18s)
        // a 3-line CJK verse hit the 4-line cap immediately and created a new shot, which
        // meant the camera + layout changed every ~4-6 seconds — far too often. Bumped
        // to 6 lines / 10s per shot / 30s per paragraph so a typical verse fits in one
        // shot and the scene stays stable long enough to read the lyrics.
        let mut group: Vec<usize> = Vec::new();
        let mut group_start: f32 = lines[ps].start_ms as f32 / 1000.0;
        let mut shot_idx_in_para = 0usize;
        for idx in ps..pe {
            let end = line_end(&lines[idx], lines.get(idx + 1));
            let fits = group.len() < 6 && (end - group_start) <= 10.0 && (end - lines[ps].start_ms as f32 / 1000.0) <= 30.0;
            if group.is_empty() || fits {
                group.push(idx);
            } else {
                let first_shot = shots.is_empty();
                let word_count: usize = group.iter().map(|&g| lines[g].text.split_whitespace().count()).sum();
                let forced = if first_shot && is_breath && word_count <= 2 { Some(ShotKind::QuietTableau) } else { None };
                push_shot(&mut shots, &mut prev_kind, &mut prev_transition, &group, lines, forced, is_chorus, seed, para_idx, shot_idx_in_para);
                shot_idx_in_para += 1;
                group = vec![idx];
                group_start = lines[idx].start_ms as f32 / 1000.0;
            }
        }
        if !group.is_empty() {
            let word_count: usize = group.iter().map(|&g| lines[g].text.split_whitespace().count()).sum();
            // folia: forced is only the breath → quiet-tableau nudge; the chorus →
            // type-impact swap is handled inside push_shot (no extra random draw).
            let forced = if shots.is_empty() && is_breath && word_count <= 2 {
                Some(ShotKind::QuietTableau)
            } else {
                None
            };
            push_shot(&mut shots, &mut prev_kind, &mut prev_transition, &group, lines, forced, is_chorus, seed, para_idx, shot_idx_in_para);
            shot_idx_in_para += 1;
        }
    }
    // Paragraph-level transitions: one per paragraph boundary, alternating kinds (folia).
    // Window follows folia's shot formula: min(0.24, max(0.14, gap*0.18)).
    let mut para_transitions: Vec<(f32, f32, TransitionKind, f32)> = Vec::with_capacity(para_bounds.len());
    let mut prev_pt: Option<TransitionKind> = None;
    for i in 0..para_bounds.len() {
        let (ps, pe) = para_bounds[i];
        let pend = line_end(&lines[pe - 1], None);
        let next_start = para_bounds
            .get(i + 1)
            .map(|&(ns, _)| lines[ns].start_ms as f32 / 1000.0)
            .unwrap_or(pend);
        let kind = choose_transition_no_repeat(seed, i as u64, prev_pt);
        prev_pt = Some(kind);
        let gap = next_start - pend;
        let window = if para_bounds.get(i + 1).is_some() {
            (gap * 0.5).clamp(0.16, 0.3)
        } else {
            0.2
        };
        para_transitions.push((lines[ps].start_ms as f32 / 1000.0, next_start, kind, window));
    }
    Program { shots, paras: para_transitions }
}

fn push_shot(
    shots: &mut Vec<Shot>,
    prev_kind: &mut Option<ShotKind>,
    prev_transition: &mut Option<TransitionKind>,
    group: &[usize],
    lines: &[crate::lyrics::LyricLine],
    forced: Option<ShotKind>,
    chorus: bool,
    seed: u64,
    para_idx: usize,
    shot_idx: usize,
) {
    let start = lines[group[0]].start_ms as f32 / 1000.0;
    // Shot-boundary transition window (folia: min(0.24, max(0.14, gap*0.18))).
    let gap = shots.last().map(|s| start - s.start).unwrap_or(0.0);
    let twindow = (gap * 0.18).clamp(0.14, 0.24);
    let end = line_end(&lines[*group.last().unwrap()], None);
    // folia chooseWithoutRepeat is stateless: a hash over `"{seed}:{para}:{shot}:{sig}"
    // decides the kind, linearly probing forward past the previous kind. We mirror that so
    // the same line at the same passage always renders the same shot across rebuilds.
    let signature = group.iter().map(|&g| lines[g].text.as_str()).collect::<Vec<_>>().join("|");
    let sig_seed = format!("{seed}:{para_idx}:{shot_idx}:{signature}");
    let mut kind = forced.unwrap_or_else(|| choose_shot_kind(&sig_seed, *prev_kind));
    // folia chorus override: a quiet-tableau pick in a chorus becomes type-impact.
    if chorus && kind == ShotKind::QuietTableau {
        kind = ShotKind::TypeImpact;
    }
    *prev_kind = Some(kind);
    let variant = hash_sonnet_seed(&format!("{seed}:{para_idx}:{shot_idx}:variant")) % 4;
    // folia transition pick — chooseWithoutRepeat over the 3 transition kinds.
    let transition = choose_transition_kind(&format!("{seed}:{para_idx}:{shot_idx}:transition"), *prev_transition);
    *prev_transition = Some(transition);
    // folia camera random unpacking: one 32-bit FNV draw, spread across 4 bytes.
    let random = hash_sonnet_seed(&format!("{seed}:{para_idx}:{shot_idx}:camera"));
    let (zoom_base, zoom_span) = match kind {
        ShotKind::PosterBlocks => (1.02, 0.16),
        ShotKind::QuietTableau => (1.12, 0.2),
        _ => (1.22, 0.26),
    };
    let r_x = ((random & 255) as f32 / 255.0 - 0.5) * 0.18;
    let r_y = (((random >> 8) & 255) as f32 / 255.0 - 0.5) * 0.14;
    // folia zoomRandom = ((random >>> 16) & 255) / 255 — a 0..1 value (the previous port
    // divided by 255 twice, freezing zoom at `zoom_base` and wasting the span). Fixed.
    let r_z = ((random >> 16) & 255) as f32 / 255.0;
    let r_rot = (((random >> 24) & 255) as f32 / 255.0 - 0.5) * 0.08;
    let camera = Camera {
        tx: r_x,
        ty: r_y,
        zoom: zoom_base + r_z * zoom_span,
        rot: r_rot,
    };
    shots.push(Shot { start, end, kind, variant, transition, twindow, line_range: group[0]..group[group.len() - 1] + 1, camera });
}

/// Paragraph-level transition kind (folia `resolveBoundaryKind` is a hash-seeded
/// kind pick; we follow `chooseWithoutRepeat` to avoid repeating the previous
/// paragraph's exit transition). Stateless FNV-1a over `"{seed}:para:{idx}"`.
fn choose_transition_no_repeat(seed: u64, idx: u64, prev: Option<TransitionKind>) -> TransitionKind {
    choose_transition_kind(&format!("{seed}:para:{idx}"), prev)
}

/// folia `chooseWithoutRepeat` over the shot kinds.
fn choose_shot_kind(sig_seed: &str, prev: Option<ShotKind>) -> ShotKind {
    let start = (hash_sonnet_seed(sig_seed) as usize) % SHOT_KINDS.len();
    for k in 0..SHOT_KINDS.len() {
        let c = SHOT_KINDS[(start + k) % SHOT_KINDS.len()];
        if Some(c) != prev {
            return c;
        }
    }
    SHOT_KINDS[start]
}

/// folia `chooseWithoutRepeat` over the 3 transition kinds.
fn choose_transition_kind(sig_seed: &str, prev: Option<TransitionKind>) -> TransitionKind {
    const KINDS: [TransitionKind; 3] = [
        TransitionKind::MonoGlitch,
        TransitionKind::FastBlur,
        TransitionKind::CameraPull,
    ];
    let start = (hash_sonnet_seed(sig_seed) as usize) % KINDS.len();
    for k in 0..KINDS.len() {
        let c = KINDS[(start + k) % KINDS.len()];
        if Some(c) != prev {
            return c;
        }
    }
    KINDS[start]
}

fn find_shot(program: &Program, t: f32) -> Option<usize> {
    let mut idx = None;
    for (i, s) in program.shots.iter().enumerate() {
        if t >= s.start {
            idx = Some(i);
        }
    }
    idx
}

/// The word whose timing window contains `time`, or the nearest one.
fn word_at(placements: &[Placement], time: f32) -> Option<usize> {
    placements
        .iter()
        .position(|p| time >= p.start && time <= p.end)
        .or_else(|| {
            placements
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (a.start - time).abs().min((a.end - time).abs());
                    let db = (b.start - time).abs().min((b.end - time).abs());
                    da.partial_cmp(&db).unwrap()
                })
                .map(|(i, _)| i)
        })
}

/// Per-placement glyph trail in the same geometry as `push_word_full` (final layout
/// positions, no entry offsets): (x, y, startTime) per visible character.
fn glyph_track_points(ctx: &StyleCtx, p: &Placement) -> Vec<(f32, f32, f32)> {
    let chars: Vec<char> = p.text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let weight = match p.role {
        Role::Hero | Role::SemiHero => 2u8,
        Role::Support => 1u8,
        Role::Decoration => 3u8,
    };
    let s = p.size / RASTER_PX;
    let cell_px = CELL as f32 * s;
    let placed = ctx.atlas.layout(&p.text, p.size, weight);
    let (pen_x, pen_y) = if p.vertical {
        (p.x - p.size * 1.4 * 0.5, p.y - p.h * 0.5)
    } else {
        (p.x - p.w / 2.0, p.y + p.size * 0.35)
    };
    let dur = (p.end - p.start).max(0.08);
    let mut out = Vec::with_capacity(placed.len());
    for (ci, gp) in placed.iter().enumerate() {
        let Some(info) = ctx.atlas.glyph(gp.ch, weight) else {
            continue;
        };
        let (qx0, qy0) = if p.vertical {
            (
                pen_x,
                pen_y + ci as f32 * p.size * 0.9 + (info.ymin - PAD as f32) * s,
            )
        } else {
            (
                pen_x + gp.start + (info.xmin - PAD as f32) * s,
                pen_y + (info.ymin - PAD as f32) * s,
            )
        };
        // Same per-char entry clock as `char_fly` (folia grapheme timing, evenly spread).
        let st = p.start + dur * (ci as f32 / chars.len() as f32);
        out.push((qx0 + cell_px * 0.5, qy0 + cell_px * 0.5, st));
    }
    out
}

/// Per-placement camera focus, interpolating along the glyph trail (folia
/// resolveSonnetSegmentCameraFocus): segment centre ± 50% pull toward the sung glyph.
fn segment_focus(ctx: &StyleCtx, p: &Placement, time: f32) -> (f32, f32) {
    let glyphs = glyph_track_points(ctx, p);
    let first = *glyphs.first().unwrap_or(&(p.x, p.y, p.start));
    let last = *glyphs.last().unwrap_or(&(p.x, p.y, p.end));
    let cx = (first.0 + last.0) * 0.5;
    let cy = (first.1 + last.1) * 0.5;
    let apply = |x: f32, y: f32| (cx + (x - cx) * 0.5, cy + (y - cy) * 0.5);
    if time <= first.2 {
        return apply(first.0, first.1);
    }
    if time >= last.2 {
        return apply(last.0, last.1);
    }
    for w in glyphs.windows(2) {
        let (cur, nxt) = (w[0], w[1]);
        if time < cur.2 || time > nxt.2 {
            continue;
        }
        let prog = (time - cur.2) / (nxt.2 - cur.2).max(0.001);
        return apply(
            cur.0 + (nxt.0 - cur.0) * prog,
            cur.1 + (nxt.1 - cur.1) * prog,
        );
    }
    apply(first.0, first.1)
}

/// Gaussian focus weights over all word windows (folia resolveSonnetFocusWeights, σ=0.35).
fn gaussian_focus(ctx: &StyleCtx, placements: &[Placement], time: f32) -> (f32, f32) {
    let sigma = 0.35f32;
    let mut fx = 0.0f32;
    let mut fy = 0.0f32;
    let mut den = 0.0f32;
    for p in placements {
        if p.giant || p.role == Role::Decoration {
            continue;
        }
        let d = if time < p.start {
            p.start - time
        } else if time > p.end {
            time - p.end
        } else {
            0.0
        };
        let w = (-(d * d) / (2.0 * sigma * sigma)).exp();
        let (px, py) = segment_focus(ctx, p, time);
        fx += px * w;
        fy += py * w;
        den += w;
    }
    if den > 1e-6 {
        (fx / den, fy / den)
    } else {
        (0.0, 0.0)
    }
}

/// Deterministic temporal smoothing of the sung-word focus (folia's 5-sample kernel), then
/// Gaussian word weighting; edge-preserving so composition jumps stay intentional.
fn resolved_focus(ctx: &StyleCtx, placements: &[Placement], t: f32) -> (f32, f32) {
    let samples: [(f32, f32); 5] = [(-0.12, 1.0), (-0.06, 4.0), (0.0, 6.0), (0.06, 4.0), (0.12, 1.0)];
    let mut fx = 0.0f32;
    let mut fy = 0.0f32;
    let mut tw = 0.0f32;
    let center = gaussian_focus(ctx, placements, t);
    for (off, wgt) in samples {
        let (px, py) = gaussian_focus(ctx, placements, t + off);
        // Edge-preserving: skip samples that jump far from the center focus.
        let dist_sq = (px - center.0) * (px - center.0) + (py - center.1) * (py - center.1);
        if dist_sq > 96.0 * 96.0 {
            continue;
        }
        fx += px * wgt;
        fy += py * wgt;
        tw += wgt;
    }
    if tw > 0.0 {
        (fx / tw, fy / tw)
    } else {
        center
    }
}

// ---------------------------------------------------------------- placement

#[derive(Debug, Clone)]
struct Placement {
    text: String,
    role: Role,
    size: f32,
    w: f32,
    h: f32,
    x: f32,
    y: f32,
    rotation: f32,
    enter: [f32; 2],
    start: f32,
    end: f32,
    /// Vertical CJK column (non-CJK segments rotate 90° instead).
    vertical: bool,
    /// Giant background echo of a hero word (folia decoration copies).
    giant: bool,
}

/// Per-template hero / support font scales — folia sonnetTypographyLayout exact values.
fn template_scales(kind: ShotKind) -> (f32, f32) {
    match kind {
        ShotKind::EditorialColumn => (4.0, 1.2),
        ShotKind::TypeImpact => (5.5, 1.5),
        ShotKind::FragmentCollage => (3.2, 1.35),
        ShotKind::TrackingRibbon => (3.5, 1.5),
        ShotKind::MaskReveal => (4.5, 1.6),
        ShotKind::PosterBlocks => (4.4, 1.15),
        ShotKind::QuietTableau => (3.0, 1.15),
    }
}

/// Word "score" = fola scoreSonnetHeroSegment (visible length + duration).
fn segment_score(text: &str, start: f32, end: f32) -> f32 {
    let visible = text.chars().filter(|c| !c.is_whitespace()).count() as f32;
    (visible.min(8.0) * 14.0) + ((end - start).min(2.5).max(0.0) * 18.0)
}

fn role_font_scale(role: Role, hero_scale: f32, support_scale: f32) -> f32 {
    match role {
        Role::Hero => hero_scale,
        Role::SemiHero => (support_scale * 1.35).max(hero_scale * 0.72),
        Role::Support => support_scale,
        Role::Decoration => (hero_scale * 0.5).max(0.6),
    }
}

fn build_placements(
    ctx: &StyleCtx,
    lines: &[crate::lyrics::LyricLine],
    shot: &Shot,
    base: f32,
    seed: u64,
) -> Vec<Placement> {
    let mut rng = Seeded::new(seed);
    let mut out: Vec<Placement> = Vec::new();
    let mut all: Vec<(String, f32, f32, usize)> = Vec::new(); // (text,start,end,line_idx)
    for li in shot.line_range.clone() {
        let line = &lines[li];
        let timing = LineTiming {
            start: line.start_ms as f32 / 1000.0,
            end: line_end(line, lines.get(li + 1)),
            duration: (line_end(line, lines.get(li + 1)) - line.start_ms as f32 / 1000.0).max(0.1),
        };
        for w in split_with_timing(line, &timing) {
            all.push((w.text, w.start, w.end, li));
        }
    }
    if all.is_empty() {
        return out;
    }
    let shot_words = all.len();
    // Global hero: the word with the highest segment score across the shot
    // (folia findSonnetHeroSegmentIndex on all segments).
    let mut hero = 0usize;
    let mut best = f32::MIN;
    for (i, (text, start, end, _)) in all.iter().enumerate() {
        let s = segment_score(text, *start, *end);
        if s > best {
            best = s;
            hero = i;
        }
    }
    // Per-line local heroes (folia heroIndices): each line's best-scored word is styled as a
    // hero too — multi-hero compositions. The layout anchors on the global hero.
    let mut hero_indices: Vec<usize> = Vec::new();
    {
        let mut line_words: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
        for (i, (_, _, _, li)) in all.iter().enumerate() {
            line_words.entry(*li).or_default().push(i);
        }
        for (_li, indices) in line_words {
            if let Some(&b) = indices
                .iter()
                .max_by(|a, b| segment_score(&all[**a].0, all[**a].1, all[**a].2).partial_cmp(&segment_score(&all[**b].0, all[**b].1, all[**b].2)).unwrap())
            {
                hero_indices.push(b);
            }
        }
    }
    // Semi-heroes: shot-wide word count ≥4 (folia SEMI_HERO_MIN_LINE_WORDS), score ≥ hero*0.35,
    // ≥2 away, on the opposite side of the hero's lean; a second one at ≥9 words.
    let word_like_count = all
        .iter()
        .filter(|(text, _, _, _)| text.chars().any(|c| c.is_alphanumeric()))
        .count();
    let hero_score = segment_score(&all[hero].0, all[hero].1, all[hero].2);
    let threshold = hero_score * 0.35;
    let hero_leans_early = hero <= (all.len() - 1) / 2;
    let semi: Vec<usize> = if word_like_count >= 4 {
        all.iter()
            .enumerate()
            .filter(|(i, (text, start, end, _))| {
                *i != hero
                    && !text.chars().all(|c| !c.is_alphanumeric())
                    && text.chars().filter(|c| !c.is_whitespace()).count() >= 2
                    && i.abs_diff(hero) >= 2
                    && segment_score(text, *start, *end) >= threshold
                    && if hero_leans_early { *i > hero } else { *i < hero }
            })
            .map(|(i, _)| i)
            .collect()
    } else {
        Vec::new()
    };
    let mut semi_heroes: Vec<usize> = Vec::new();
    let best_of = |list: &[usize]| -> Option<usize> {
        list.iter().copied().max_by(|a, b| {
            segment_score(&all[*a].0, all[*a].1, all[*a].2).partial_cmp(&segment_score(&all[*b].0, all[*b].1, all[*b].2)).unwrap()
        })
    };
    let primary_side: Vec<usize> = semi.iter().copied().filter(|i| if hero_leans_early { *i > hero } else { *i < hero }).collect();
    let secondary_side: Vec<usize> = semi.iter().copied().filter(|i| if hero_leans_early { *i < hero } else { *i > hero }).collect();
    // Primary side ?? fallback to the opposite side (folia sonnetTypographyRoles.ts:100).
    if let Some(p) = best_of(&primary_side).or_else(|| best_of(&secondary_side)) {
        semi_heroes.push(p);
        if word_like_count >= 9 {
            if let Some(q) = best_of(&semi).filter(|&q| q != p && q.abs_diff(p) >= 2) {
                semi_heroes.push(q);
            }
        }
    }

    let (hero_scale, support_scale) = template_scales(shot.kind);
    for (i, (text, start, end, _)) in all.into_iter().enumerate() {
        let role = if i == hero || hero_indices.contains(&i) {
            Role::Hero
        } else if semi_heroes.contains(&i) {
            Role::SemiHero
        } else if rng.unit() < 0.18 {
            Role::Decoration
        } else {
            Role::Support
        };
        let size = base * role_font_scale(role, hero_scale, support_scale);
        let bold = matches!(role, Role::Hero | Role::SemiHero);
        let is_cjk = is_cjk_text(&text);
        let column_template = matches!(shot.kind, ShotKind::MaskReveal | ShotKind::EditorialColumn | ShotKind::FragmentCollage);
        // Vertical CJK columns for emphasis words in column-oriented templates (folia);
        // non-CJK emphasis words there rotate 90° as a block (folia shouldRotateNonCjkSegment).
        let vertical = is_cjk && column_template && matches!(role, Role::Hero | Role::SemiHero);
        let rotates = !is_cjk && column_template && matches!(role, Role::Hero | Role::SemiHero) && text.chars().count() > 1;
        let (w, h) = if vertical || rotates {
            (size * 1.4, text.chars().count().max(1) as f32 * size * 0.9)
        } else {
            let w = if matches!(role, Role::Hero | Role::SemiHero) {
                measure_text_bold(ctx.atlas, &text, size)
            } else {
                measure_text(ctx.atlas, &text, size)
            };
            (w, size * 1.2)
        };
        let rotation = if rotates { std::f32::consts::FRAC_PI_2 } else { 0.0 };
        out.push(Placement { text, role, size, w, h, x: 0.0, y: 0.0, rotation, enter: [0.0, 0.0], start, end, vertical: vertical || rotates, giant: false });
    }
    // Giant background echoes of hero words (folia's decoration copies, 2.8-5.5×). Skipped
    // on crowded shots to keep the per-pixel shader loop bounded.
    let crowd = shot_words > 12;
    if !crowd && !matches!(shot.kind, ShotKind::QuietTableau | ShotKind::PosterBlocks) {
        let heroes: Vec<(String, f32, f32)> = out.iter().filter(|p| p.role == Role::Hero).map(|p| (p.text.clone(), p.start, p.end)).collect();
        let heroes_clone: Vec<Placement> = heroes
            .iter()
            .enumerate()
            .map(|(idx, (text, start, end))| {
                // folia decoration copies: previously 2.8–5.5× the hero — way too big, the
                // giant letters swamped the screen and cost a lot per frame to rasterize.
                // Pulled down to 1.4–2.2× so they read as ambient decoration, not text.
                let size = base * (hero_scale * 1.35).max(1.4).min(2.2);
                let w = measure_text(ctx.atlas, text, size);
                Placement {
                    text: text.clone(),
                    role: Role::Decoration,
                    size,
                    w,
                    h: size * 1.2,
                    x: -ctx.width * (0.10 - idx as f32 * 0.03),
                    y: -ctx.height * (0.05 - idx as f32 * 0.02),
                    rotation: -0.15 + if idx % 2 == 0 { 0.0 } else { 0.05 },
                    enter: [-ctx.width * 0.05, -ctx.height * 0.05],
                    start: *start,
                    end: *end,
                    vertical: false,
                    giant: true,
                }
            })
            .collect();
        // Insert giants at the back (behind everything) — order irrelevant for additive blend.
        for g in heroes_clone {
            out.push(g);
        }
        // dec2: a second, smaller echo of a non-hero word near the hero
        // (folia clamp(fontScale*2.2, 1.8, 3.5)).
        if let Some(first_non_hero) = out.iter().find(|q| q.role != Role::Hero && !q.giant) {
            if let Some(first_hero) = out.iter().find(|q| q.role == Role::Hero) {
                // dec2: a second, smaller echo of a non-hero word near the hero
                // (folia clamp(fontScale*2.2, 1.8, 3.5) — also pulled down to match).
                let size = base * (hero_scale * 2.2).max(1.1).min(1.6);
                let w = measure_text(ctx.atlas, &first_non_hero.text, size);
                out.push(Placement {
                    text: first_non_hero.text.clone(),
                    role: Role::Decoration,
                    size,
                    w,
                    h: size * 1.2,
                    x: first_hero.x + ctx.width * 0.25,
                    y: first_hero.y + ctx.height * 0.15,
                    rotation: 0.08,
                    enter: [ctx.width * 0.05, ctx.height * 0.05],
                    start: first_non_hero.start,
                    end: first_non_hero.end,
                    vertical: false,
                    giant: true,
                });
            }
        }
    }
    out
}

fn is_cjk_text(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}'))
}

#[allow(dead_code)]
fn _vertical_rotation(p: &Placement) -> f32 {
    if p.vertical && !is_cjk_text(&p.text) {
        std::f32::consts::FRAC_PI_2
    } else {
        0.0
    }
}

/// Global-fit: shrink all placements so they fit the safe area (measured boxes, no overlap by
/// construction of each template; this only guards overflow).
fn fit_extent(placements: &[Placement], w: f32, h: f32, cam_zoom: f32) -> f32 {
    // folia: per-box 82% screen pre-fit, then a global 7-step scale retry (1, 0.92 … 0.52)
    // against the 48%/46% safe area. Giant echoes overflow freely.
    // The lyric layer is scaled by `shot.zoom * path_scale` (≤ ~1.31) around the screen
    // centre, so the safe area must shrink by that factor or the zoom pushes lyrics off the
    // edges. The MG background layer gets its own weaker transform (0.3 blend + parallax).
    let cam = (cam_zoom * 1.35).max(1.0);
    let safe_hw = w * 0.48 / cam;
    let safe_hh = h * 0.46 / cam;
    let fits = |f: f32| -> bool {
        for p in placements {
            if p.giant {
                continue;
            }
            let rx = p.x.abs() * f + p.w * f / 2.0;
            let ry = p.y.abs() * f + p.h * f / 2.0;
            if rx > safe_hw || ry > safe_hh {
                return false;
            }
        }
        true
    };
    for k in 0..7u32 {
        let f = 1.0 - k as f32 * 0.08;
        if fits(f) {
            return f;
        }
    }
    0.52
}

fn set_enter(p: &mut Placement, dx: f32, dy: f32) {
    p.enter = [dx, dy];
}

// ---------------------------------------------------------------- layouts

fn cross_fill_column(p: &mut [Placement], idx: &[usize], hero: usize, hero_scale: f32, base_sz: f32, h: f32, sgap: f32, start_y: f32, downward: bool, enter_dy: f32) {
    if idx.is_empty() {
        return;
    }
    // folia fillColumn: boost a column's words so it fills 72% of the free band, capped at
    // 2.2x or at hero*0.6/wordScale, then justify the remaining slack as extra pitch.
    let available = (h * 0.46 - p[hero].h / 2.0 - sgap).max(0.0);
    let gaps = sgap * (idx.len() as f32 - 1.0);
    let content: f32 = idx.iter().map(|&i| p[i].h).sum();
    if available > 0.0 && content + gaps < available * 0.72 {
        let boost = ((available * 0.72 - gaps) / content.max(1.0)).min(2.2);
        for &i in idx {
            let capped = boost.min((hero_scale * 0.6) / (p[i].size / base_sz).max(0.01));
            if capped > 1.05 {
                p[i].size *= capped;
                p[i].w *= capped;
                p[i].h *= capped;
            }
        }
    }
    let grown: f32 = idx.iter().map(|&i| p[i].h).sum();
    let pitch = if idx.len() > 1 {
        ((available * 0.95 - grown) / (idx.len() as f32 - 1.0)).max(0.0).min(sgap * 2.0)
    } else {
        0.0
    };
    let mut cy = start_y;
    for (k, &i) in idx.iter().enumerate() {
        let kk = k as f32;
        let y = if downward { cy + p[i].h / 2.0 } else { cy - p[i].h / 2.0 };
        p[i].x = if i % 2 == 0 { 14.0 } else { -14.0 };
        p[i].y = y + if downward { kk * pitch } else { -(kk * pitch) };
        p[i].rotation = 0.0;
        set_enter(&mut p[i], 0.0, enter_dy);
        cy = y;
        if downward {
            cy += p[i].h / 2.0 + sgap;
        } else {
            cy -= p[i].h / 2.0 + sgap;
        }
    }
}

fn layout_cross_stack(p: &mut [Placement], w: f32, h: f32) {
    // Dynamic cross (folia layoutCrossStack): top column -> left row -> hero -> right row ->
    // bottom column, with fillColumn band-filling for the two columns.
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    let before_count = hero;
    let top_count = before_count / 2;
    let after_count = p.len() - 1 - hero;
    let right_count = after_count.div_ceil(2);
    let base_sz = p[hero].size.max(40.0);
    let gap = (base_sz * 0.35).clamp(16.0, 40.0);
    let sgap = (gap * 1.35).max(24.0);
    let hero_scale = p[hero].size / base_sz;
    let hero_box = hero;
    p[hero_box].x = 0.0;
    p[hero_box].y = 0.0;
    set_enter(&mut p[hero_box], 0.0, 0.0);

    let top_idx: Vec<usize> = (0..top_count).collect();
    cross_fill_column(p, &top_idx, hero, hero_scale, base_sz, h, sgap, p[hero_box].y - p[hero_box].h / 2.0 - sgap, false, -34.0);
    let bottom_idx: Vec<usize> = ((hero + right_count + 1)..p.len()).collect();
    cross_fill_column(p, &bottom_idx, hero, hero_scale, base_sz, h, sgap, p[hero_box].y + p[hero_box].h / 2.0 + sgap, true, 34.0);

    // Left row: topCount..heroIndex-1.
    let mut cx = -(p[hero_box].w / 2.0 + gap + p[top_count.min(hero)].w / 2.0);
    for i in (top_count..hero).rev() {
        p[i].x = cx;
        p[i].y = if i % 2 == 0 { 12.0 } else { -12.0 };
        p[i].rotation = 0.0;
        set_enter(&mut p[i], -34.0, 0.0);
        cx -= p[i].w + gap;
    }
    // Right row: heroIndex+1 .. heroIndex+rightCount.
    cx = p[hero_box].w / 2.0 + gap + p[(hero + 1).min(p.len() - 1)].w / 2.0;
    for i in (hero + 1)..=(hero + right_count).min(p.len() - 1) {
        p[i].x = cx;
        p[i].y = if i % 2 == 0 { 12.0 } else { -12.0 };
        p[i].rotation = 0.0;
        set_enter(&mut p[i], 34.0, 0.0);
        cx += p[i].w + gap;
    }
}

fn layout_quiet_tableau(p: &mut [Placement], w: f32, h: f32, variant: u32) {
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    let base_sz = p[hero].size.max(40.0);
    let gap = (base_sz * 0.35).clamp(16.0, 40.0);
    let sgap = (gap * 1.35).max(24.0);
    // folia layoutQuietTableau: v2/v3 are "horizontal cards" (layout direction only),
    // v3 adds ±35 stagger; v0/v1 hero sits at -0.1h, v2/v3 at 0.
    let horizontal_card = variant == 2 || variant == 3;
    p[hero].x = 0.0;
    p[hero].y = if horizontal_card { 0.0 } else { -h * 0.1 };
    set_enter(&mut p[hero], 0.0, 0.0);
    let stagger = if variant == 3 { 70.0 } else { 0.0 };
    let safe_hh = h * 0.46;
    let max_w = p.iter().map(|x| x.w).fold(0.0f32, f32::max);
    let column_step = max_w + sgap + stagger;
    let hx = p[hero].x;
    let hw = p[hero].w;
    let x_for = |p: &[Placement], i: usize| -> f32 {
        match variant {
            1 => hx - hw / 2.0 + p[i].w / 2.0,
            3 => hx + if i % 2 == 0 { 1.0 } else { -1.0 } * 35.0,
            _ => hx,
        }
    };
    // Before run: upward from the hero; overflow wraps into columns marching right.
    let mut column = 0i32;
    let mut cy = p[hero].y - p[hero].h / 2.0 - sgap;
    for i in (0..hero).rev() {
        if cy - p[i].h < -safe_hh {
            column += 1;
            cy = safe_hh;
        }
        p[i].x = x_for(p, i) + column as f32 * column_step;
        p[i].y = cy - p[i].h / 2.0;
        cy -= p[i].h + sgap;
        if variant == 1 {
            set_enter(&mut p[i], 20.0, 0.0);
        } else if variant == 3 {
            let ex = if p[i].x > hx { 30.0 } else { -30.0 };
            set_enter(&mut p[i], ex, 0.0);
        } else {
            set_enter(&mut p[i], 0.0, 20.0);
        }
    }
    // After run: downward from the hero; overflow wraps into columns marching left.
    column = 0;
    cy = p[hero].y + p[hero].h / 2.0 + sgap;
    for i in (hero + 1)..p.len() {
        if cy + p[i].h > safe_hh {
            column += 1;
            cy = -safe_hh;
        }
        p[i].x = x_for(p, i) - column as f32 * column_step;
        p[i].y = cy + p[i].h / 2.0;
        cy += p[i].h + sgap;
        if variant == 1 {
            set_enter(&mut p[i], -20.0, 0.0);
        } else if variant == 3 {
            let ex = if p[i].x > hx { 30.0 } else { -30.0 };
            set_enter(&mut p[i], ex, 0.0);
        } else {
            set_enter(&mut p[i], 0.0, -20.0);
        }
    }
    let _ = (horizontal_card, stagger);
}

fn layout_tracking_ribbon(p: &mut [Placement], w: f32, h: f32, variant: u32) {
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    let base_sz = p[hero].size.max(40.0);
    let gap = (base_sz * 0.35).clamp(16.0, 40.0);
    p[hero].x = 0.0;
    p[hero].y = 0.0;
    set_enter(&mut p[hero], 0.0, 0.0);
    let hero_h = p[hero].h;
    let y_for = |i: usize, hi: f32| -> f32 {
        match variant % 3 {
            1 => hero_h / 2.0 - hi / 2.0, // bottom-aligned (folia v1: +h/2-hi/2)
            2 => -hero_h / 2.0 + hi / 2.0, // top-aligned (folia v2: -h/2+hi/2)
            _ => if i % 2 == 0 { 10.0 } else { -10.0 }, // zigzag
        }
    };
    let enter = if variant % 3 == 2 { 20.0 } else { 30.0 };
    let mut cx = -(p[hero].w / 2.0 + gap);
    for i in (0..hero).rev() {
        p[i].x = cx - p[i].w / 2.0;
        let hi = p[i].h;
        p[i].y = y_for(i, hi);
        set_enter(&mut p[i], enter, 0.0);
        cx -= p[i].w + gap;
    }
    cx = p[hero].w / 2.0 + gap;
    for i in (hero + 1)..p.len() {
        p[i].x = cx + p[i].w / 2.0;
        let hi = p[i].h;
        p[i].y = y_for(i, hi);
        set_enter(&mut p[i], -enter, 0.0);
        cx += p[i].w + gap;
    }
}

fn layout_editorial_column(p: &mut [Placement], w: f32, h: f32, variant: u32) {
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    // folia spacing: flowGap = clamp(base*0.35, 16, 40), stackGap = max(24, flowGap*1.35).
    let base_sz = p[hero].size.max(40.0);
    let gap = (base_sz * 0.35).clamp(16.0, 40.0);
    let sgap = (gap * 1.35).max(24.0);
    let safe_l = -w * 0.42;
    let safe_r = w * 0.42;
    match variant % 5 {
        0 => {
            // Hero pillar on the left; earlier words in the right column, later in the left.
            p[hero].x = -w * 0.15;
            p[hero].y = 0.0;
            set_enter(&mut p[hero], 0.0, 0.0);
            let mut cy = -h * 0.4 + p[hero].h / 2.0;
            for i in 0..hero {
                p[i].x = p[hero].x + p[hero].w / 2.0 + gap + p[i].w / 2.0;
                p[i].y = cy;
                set_enter(&mut p[i], -20.0, 0.0);
                cy += p[i].h + gap;
            }
            cy = -h * 0.4 + p[hero].h / 2.0;
            for i in (hero + 1)..p.len() {
                p[i].x = p[hero].x - p[hero].w / 2.0 - gap - p[i].w / 2.0;
                p[i].y = cy;
                set_enter(&mut p[i], 20.0, 0.0);
                cy += p[i].h + gap;
            }
        }
        1 => {
            // Flush-right magazine rail (folia v1): single centered rail when it fits at
            // 52% scale, otherwise rails marching left (columns read right-to-left).
            let right_edge = w * 0.28;
            let safe_hh = h * 0.46;
            let rail_max_w = p.iter().map(|x| x.w).fold(0.0f32, f32::max);
            let rail_step = rail_max_w + sgap;
            let total_h: f32 = p.iter().map(|x| x.h).sum::<f32>() + sgap * (p.len() as f32 - 1.0);
            let fits_single = total_h * 0.52 + sgap * (p.len() as f32 - 1.0) <= safe_hh * 2.0;
            if fits_single {
                let mut cy = -total_h / 2.0;
                for i in 0..p.len() {
                    p[i].x = right_edge - p[i].w / 2.0;
                    p[i].y = cy + p[i].h / 2.0;
                    cy += p[i].h + sgap;
                    set_enter(&mut p[i], 20.0, 0.0);
                }
            } else {
                let mut rail = 0i32;
                let mut cy = -safe_hh;
                for i in 0..p.len() {
                    if cy + p[i].h > safe_hh {
                        rail += 1;
                        cy = -safe_hh;
                    }
                    p[i].x = (right_edge - rail as f32 * rail_step) - p[i].w / 2.0;
                    p[i].y = cy + p[i].h / 2.0;
                    cy += p[i].h + sgap;
                    set_enter(&mut p[i], 20.0, 0.0);
                }
            }
        }
        2 => {
            // Magazine header: kicker row above the hero, paired columns below.
            p[hero].x = 0.0;
            p[hero].y = -h * 0.24;
            set_enter(&mut p[hero], 0.0, 0.0);
            let before: Vec<usize> = (0..hero).collect();
            let after: Vec<usize> = ((hero + 1)..p.len()).collect();
            if !before.is_empty() {
                let kw: f32 = before.iter().map(|&i| p[i].w).sum::<f32>() + gap * (before.len() - 1) as f32;
                let ky = p[hero].y - p[hero].h / 2.0 - gap - before.iter().map(|&i| p[i].h).fold(0.0f32, f32::max) / 2.0;
                let mut cx = -kw / 2.0;
                for &i in &before {
                    p[i].x = cx + p[i].w / 2.0;
                    p[i].y = ky;
                    set_enter(&mut p[i], 0.0, -22.0);
                    cx += p[i].w + gap;
                }
            }
            let left = p[hero].x - p[hero].w * 0.3 - gap;
            let right = p[hero].x + p[hero].w * 0.3 + gap;
            let mut cy = p[hero].y + p[hero].h / 2.0 + gap;
            for (k, &i) in after.iter().enumerate() {
                let pair = k % 2 == 0;
                p[i].x = if pair { left - p[i].w / 2.0 } else { right + p[i].w / 2.0 };
                p[i].y = cy;
                set_enter(&mut p[i], if pair { -22.0 } else { 22.0 }, 0.0);
                if !pair {
                    cy += p[i].h + gap;
                }
            }
        }
        3 => {
            // Double hero lines (folia v3): two offset lines, each left-to-right in timeline
            // order, line1 shifted -offset and line2 +offset by max(line1W, line2W)*0.12.
            let split = hero;
            let total1: f32 = p[..split].iter().map(|x| x.w).sum::<f32>() + gap * split.saturating_sub(1) as f32;
            let total2: f32 = p[split..].iter().map(|x| x.w).sum::<f32>() + gap * (p.len() - split).saturating_sub(1) as f32;
            let hh = p.iter().map(|x| x.h).fold(0.0f32, f32::max);
            let line1_w: f32 = if split > 0 { p[..split].iter().map(|x| x.w).sum::<f32>() } else { 0.0 };
            let line2_w: f32 = p[split..].iter().map(|x| x.w).sum::<f32>();
            let offset = line1_w.max(line2_w) * 0.12;
            let line1_y = -hh / 2.0 - gap / 2.0;
            let line2_y = hh / 2.0 + gap / 2.0;
            let mut cx = -total1 / 2.0 - offset;
            for i in 0..split {
                p[i].x = cx + p[i].w / 2.0;
                p[i].y = line1_y;
                set_enter(&mut p[i], 30.0, 0.0);
                cx += p[i].w + gap;
            }
            cx = -total2 / 2.0 + offset;
            for i in split..p.len() {
                p[i].x = cx + p[i].w / 2.0;
                p[i].y = line2_y;
                set_enter(&mut p[i], -30.0, 0.0);
                cx += p[i].w + gap;
            }
        }
        4 | _ => {
            // Logo badge (folia v4): hero is the last segment; it anchors one side and the
            // supports float as a block opposite (folia flowWords/regionFor simplified).
            let hero_on_right = hero == p.len() - 1;
            let hero_x = if hero_on_right { w * 0.2 } else { -w * 0.2 };
            p[hero].x = hero_x;
            p[hero].y = 0.0;
            set_enter(&mut p[hero], 0.0, 0.0);
            let block_x = hero_x - if hero_on_right { p[hero].w / 2.0 + sgap } else { -(p[hero].w / 2.0 + sgap) };
            let mut cy = -h * 0.3;
            let supports: Vec<usize> = (0..p.len()).filter(|&i| i != hero).collect();
            for &i in &supports {
                p[i].x = block_x + if hero_on_right { -p[i].w / 2.0 } else { p[i].w / 2.0 };
                p[i].y = cy;
                set_enter(&mut p[i], if hero_on_right { 30.0 } else { -30.0 }, 0.0);
                cy += p[i].h + sgap;
            }
        }
    }
}

fn layout_fragment_collage(p: &mut [Placement], w: f32, h: f32, seed: u64, variant: u32) {
    let mut rng = Seeded::new(seed.wrapping_add(0xABC));
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    let base_sz = p[hero].size.max(40.0);
    let gap = (base_sz * 0.35).clamp(16.0, 40.0);
    let sgap = (gap * 1.35).max(24.0);
    p[hero].x = 0.0;
    p[hero].y = 0.0;
    set_enter(&mut p[hero], 0.0, 0.0);
    // Flatten rotated boxes back to horizontal (folia collage flattens rotations).
    for i in 0..p.len() {
        if i == hero {
            continue;
        }
        let quarter = ((p[i].rotation / (std::f32::consts::FRAC_PI_2)).round() as i64).rem_euclid(2);
        if quarter == 1 {
            std::mem::swap(&mut p[i].w, &mut p[i].h);
        }
        p[i].rotation = 0.0;
    }
    let hero_radius = (p[hero].w * p[hero].w + p[hero].h * p[hero].h).sqrt() / 2.0 + sgap;
    let supports: Vec<usize> = (0..p.len()).filter(|&i| i != hero).collect();
    let count = supports.len().max(1) as f32;
    let squash = 0.65;
    let mut placed: Vec<(f32, f32, f32, f32)> = vec![(-p[hero].w / 2.0, p[hero].w / 2.0, -p[hero].h / 2.0, p[hero].h / 2.0)];
    let mut angle = std::f32::consts::FRAC_PI_4;
    let mut support_index = 0usize;
    let sep = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| -> f32 {
        (a.0 - b.1).max(b.0 - a.1).max(a.2 - b.3).max(b.2 - a.3)
    };
    for &idx in &supports {
        let mut radius = hero_radius;
        radius += match variant % 3 {
            1 => (35.0 + (support_index as f32 / count) * 150.0), // spiral
            2 => if support_index % 2 == 1 { 140.0 } else { 50.0 }, // double ring
            _ => 45.0 + ((support_index as u32 * 23) % 90) as f32, // classic jitter
        };
        support_index += 1;
        let mut candidate = angle;
        let mut rect = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut resolved = radius;
        let mut clear = false;
        'outer: for ring in 0..14 {
            for _attempt in 0..400 {
                let cx = candidate.cos() * resolved;
                let cy = candidate.sin() * resolved * squash;
                rect = (cx - p[idx].w / 2.0, cx + p[idx].w / 2.0, cy - p[idx].h / 2.0, cy + p[idx].h / 2.0);
                if placed.iter().all(|o| sep(rect, *o) >= gap) {
                    clear = true;
                    break 'outer;
                }
                candidate += 0.07;
            }
            resolved += 36.0 + ring as f32 * 12.0;
        }
        if !clear {
            // fallback: place at the last candidate
        }
        angle = candidate + 0.02;
        placed.push(rect);
        p[idx].x = candidate.cos() * resolved;
        p[idx].y = candidate.sin() * resolved * squash;
        set_enter(&mut p[idx], candidate.cos() * -60.0, candidate.sin() * -60.0);
    }
    let _ = (rng.next(), w, h);
}

fn layout_poster_blocks(p: &mut [Placement], w: f32, h: f32, seed: u64) {
    let mut rng = Seeded::new(seed.wrapping_add(0x111));
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    p[hero].x = 0.0;
    p[hero].y = -h * 0.12;
    set_enter(&mut p[hero], 0.0, 0.0);
    // Flow the rest into rows above/below the hero block. The original layout put every
    // word in rows stacked above the hero with no vertical safe-area check, so any
    // Decoration/Support with size ~400–600 would land at y < -700 (safe area is ±0.46h
    // = ±669px on a 1455-tall viewport) and be clipped off-screen. Now: wrap to the
    // right side of the hero when the row walks off the top, so all non-hero words stay
    // inside the safe area.
    let gap = 22.0;
    let safe_l = -w * 0.42;
    let safe_r = w * 0.42;
    let safe_top = -h * 0.46;
    let mut row_y = p[hero].y - p[hero].h / 2.0 - gap;
    let mut cx = safe_l;
    // side: 0 = pack on the left of the hero, 1 = pack on the right. We never go past 2
    // columns — if a shot has more non-hero words than fit on both sides, fit_extent
    // below will scale them down so the whole composition still fits the safe area.
    let mut side: i32 = 0;
    for &i in (0..hero).chain(hero + 1..p.len()).collect::<Vec<_>>().iter() {
        if cx > safe_l && cx + p[i].w > safe_r {
            row_y -= p[i].h + gap;
            cx = if side == 0 { safe_l } else { safe_r - p[i].w };
        }
        // Walked off the top of the safe area: switch to the other side of the hero and
        // start a fresh row just above the hero.
        if row_y - p[i].h < safe_top {
            side = 1 - side;
            row_y = p[hero].y - p[hero].h / 2.0 - gap;
            cx = if side == 0 { safe_l } else { safe_r - p[i].w };
        }
        p[i].x = cx + p[i].w / 2.0;
        p[i].y = row_y;
        p[i].rotation = (rng.unit() - 0.5) * 0.06;
        set_enter(&mut p[i], (rng.unit() - 0.5) * 40.0, -24.0);
        cx += p[i].w + gap;
    }
}

fn layout_type_impact(p: &mut [Placement], w: f32, h: f32) {
    let hero = p.iter().position(|x| x.role == Role::Hero).unwrap_or(0);
    p[hero].x = 0.0;
    p[hero].y = -h * 0.05;
    p[hero].size *= 1.35;
    p[hero].w *= 1.35;
    p[hero].h *= 1.35;
    set_enter(&mut p[hero], 0.0, 0.0);
    let mut cy = p[hero].y + p[hero].h / 2.0 + 26.0;
    for i in 0..p.len() {
        if i == hero {
            continue;
        }
        p[i].x = 0.0;
        p[i].y = cy;
        set_enter(&mut p[i], 0.0, 20.0);
        cy += p[i].h + 18.0;
    }
}

// ---------------------------------------------------------------- camera

/// Shot camera path + gentle handheld drift + timeline shake, resolved from absolute time.
/// `breath` (0..1) ramps the handheld float in after the reveal (folia breath weight).
fn camera_frame(kind: ShotKind, progress: f32, time: f32, shot: &Camera, breath: f32) -> (f32, f32, f32, f32) {
    let lin = clamp01(progress);
    // Path easing: quick enter, near-constant drift, soft settle.
    let eased = match kind {
        ShotKind::TrackingRibbon | ShotKind::FragmentCollage | ShotKind::QuietTableau | ShotKind::PosterBlocks => {
            lin * 0.55 + ease_in_out(lin) * 0.45
        }
        _ => {
            if lin < 0.18 { ease_expo_out(lin / 0.18) * 0.22 }
            else if lin < 0.78 { 0.22 + ((lin - 0.18) / 0.6) * 0.56 }
            else { let s = (lin - 0.78) / 0.22; 0.78 + (1.0 - (1.0 - s) * (1.0 - s)) * 0.22 }
        }
    };
    // folia resolveShotMotionFrame exact per-kind coefficients.
    let (px, py, ps, pr) = match kind {
        ShotKind::EditorialColumn => (-0.055 + eased * 0.095, 0.025 - eased * 0.04, 0.98 + eased * 0.07, -0.006 + eased * 0.01),
        ShotKind::TypeImpact => {
            // folia type-impact: 22% entrance scale pull-back, then drift.
            let enter_scale = (1.0 - ease_expo_out((lin / 0.18).min(1.0))) * 0.22;
            (-0.035 + eased * 0.07, 0.018 - eased * 0.028, 1.0 + enter_scale + eased * 0.08, -0.01 + eased * 0.016)
        }
        ShotKind::FragmentCollage => (-0.045 + eased * 0.085, 0.028 - (eased * std::f32::consts::PI).sin() * 0.055, 0.97 + eased * 0.09, -0.014 + eased * 0.028),
        ShotKind::TrackingRibbon => (-0.16 + eased * 0.28, 0.05 - eased * 0.085, 0.98 + eased * 0.07, 0.008 - eased * 0.014),
        ShotKind::MaskReveal => (0.035 - eased * 0.065, 0.1 - eased * 0.135, 0.96 + eased * 0.12, -0.006 + eased * 0.009),
        ShotKind::PosterBlocks => (-0.012 + eased * 0.024, 0.008 - eased * 0.016, 0.99 + eased * 0.025, -0.0015 + eased * 0.003),
        ShotKind::QuietTableau => (-0.022 + eased * 0.04, 0.014 - eased * 0.025, 1.0 + eased * 0.028, -0.002 + eased * 0.003),
    };
    // Handheld breath (folia resolveSonnetCameraBreath): incommensurate sines with phases.
    let tau = time * std::f32::consts::TAU;
    let phase = (shot.zoom * 37.0).fract() * std::f32::consts::TAU;
    let bx = ((tau * 0.13 + phase).sin() * 0.65 + (tau * 0.31 + phase * 1.7).sin() * 0.35) * 0.006;
    let by = ((tau * 0.11 + phase * 2.3).cos() * 0.65 + (tau * 0.29 + phase * 0.9).sin() * 0.35) * 0.006;
    let bs = (tau * 0.09 + phase * 1.3).sin() * 0.002;
    let br = (tau * 0.07 + phase * 2.9).sin() * 0.0015;
    let scale = shot.zoom * (ps + bs * breath);
    let pan_x = px + bx * breath;
    let pan_y = py + by * breath;
    let rot = shot.rot + pr + br * breath;
    // folia `resolveTimelineShake` is wired in the runtime but the folia runtime passes
    // amplitude 0, so long holds sit perfectly still; drop our synthetic whisper so the
    // port matches the calm cadence.
    (scale, pan_x, pan_y, rot)
}

// ---------------------------------------------------------------- transitions

#[derive(Debug, Clone, Copy)]
struct Transition {
    alpha: f32,
    blur: f32,
    glitch: f32,
    /// Camera-pull amount (0..1): the frame pulls back from a zoom + pan toward its pose.
    pull: f32,
}

fn resolve_transition(kind: TransitionKind, phase: &str, progress: f32) -> Transition {
    let lin = clamp01(progress);
    let e = ease_in_out(lin);
    let amount = if phase == "exit" { e } else { 1.0 - e };
    match kind {
        TransitionKind::FastBlur => Transition {
            alpha: if phase == "exit" { 1.0 - amount } else { 1.0 - amount * 0.82 },
            blur: amount,
            glitch: 0.0,
            pull: 0.0,
        },
        TransitionKind::MonoGlitch => Transition {
            alpha: if phase == "exit" && lin > 0.86 { 1.0 - (lin - 0.86) / 0.14 } else { 1.0 },
            blur: 0.0,
            glitch: amount,
            pull: 0.0,
        },
        TransitionKind::CameraPull => Transition {
            // folia: camera-pull falls through to the default fade branch.
            alpha: if phase == "exit" { 1.0 - amount } else { 1.0 - amount * 0.72 },
            blur: 0.0,
            glitch: 0.0,
            pull: 0.0,
        },
    }
}

// ---------------------------------------------------------------- build_frame

pub fn build_frame(ctx: &StyleCtx, input: &StyleInput) -> StyleOutput {
    let _t_total = std::time::Instant::now();
    let _t0 = std::time::Instant::now();
    let scales = FontScales::from_height(ctx.height);
    let lines = input.lines;
    // compile_program is pure work (sorts gaps, classifies paragraphs, picks shot kinds)
    // but the result only depends on `(lines, seed)`. The lyric worker hands us a fresh
    // LyricData whenever the track changes, so keying by the slice's pointer + length
    // invalidates the cache automatically without any extra plumbing.
    let program = PROGRAM_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let key = (lines.as_ptr(), lines.len(), ctx.seed);
        if let Some(p) = cache.get(&key) {
            p.clone()
        } else {
            let p = compile_program(lines, ctx.seed);
            if cache.len() > 3 {
                cache.clear();
            }
            cache.insert(key, p.clone());
            p
        }
    });
    let _t_compile = _t0.elapsed();
    let Some(shot_idx) = find_shot(&program, ctx.time) else {
        return StyleOutput::empty();
    };
    let shot = &program.shots[shot_idx];
    let base = scales.main;
    let t = ctx.time;
    let _t1 = std::time::Instant::now();

    let cache_key = (ctx.seed, shot_idx);
    let mut placements = PLACEMENT_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if let Some(cached) = cache.get(&cache_key) {
            cached.clone()
        } else {
            let new = build_placements(ctx, lines, shot, base, ctx.seed.wrapping_add(shot_idx as u64 * 0x9E37));
            // Bound the cache to the last 8 shots to keep memory in check.
            if cache.len() > 8 {
                cache.clear();
            }
            cache.insert(cache_key, new.clone());
            new
        }
    });
    let _t_placements = _t1.elapsed();
    // Pre-warm the next shot's placements so the first frame after a transition
    // doesn't pay the full `build_placements` cost. Without this, each shot boundary
    // causes a visible frame stutter as ~10-15 placements are computed inline.
    if let Some(next_shot) = program.shots.get(shot_idx + 1) {
        let next_key = (ctx.seed, shot_idx + 1);
        PLACEMENT_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            if !cache.contains_key(&next_key) {
                let new = build_placements(
                    ctx,
                    lines,
                    next_shot,
                    base,
                    ctx.seed.wrapping_add((shot_idx + 1) as u64 * 0x9E37),
                );
                if cache.len() > 8 {
                    cache.clear();
                }
                cache.insert(next_key, new);
            }
        });
    }
    let mut _t_mg = std::time::Duration::ZERO;
    let mut _t_cam = std::time::Duration::ZERO;
    if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
        eprintln!("sonnet: shot={shot_idx} kind={:?} lines={:?} placements={}", shot.kind, shot.line_range, placements.len());
        for (i, p) in placements.iter().enumerate().take(6) {
            eprintln!("  p[{i}] role={:?} text={} size={:.0} w={:.0} x={:.0} y={:.0} start={:.1} end={:.1}", p.role, p.text, p.size, p.w, p.x, p.y, p.start, p.end);
        }
    }
    if placements.is_empty() {
        return StyleOutput::empty();
    }

    // Template layout. Giant background echoes keep their fixed placement (they are not part
    // of the flow layout); normal placements get arranged per template.
    let mut giants: Vec<Placement> = Vec::new();
    let mut normal: Vec<Placement> = Vec::new();
    for p in placements.drain(..) {
        if p.giant {
            giants.push(p);
        } else {
            normal.push(p);
        }
    }
    // Layout variant seeded from the shot's text (folia layoutVariantSeed: text lengths +
    // segment count), not the shot rng — keeps each template's variant mapping faithful.
    let variant_seed = normal.iter().map(|p| p.text.trim().len()).sum::<usize>() + normal.len();
    let e_variant = (variant_seed % 5) as u32;
    let t_variant = (variant_seed % 4) as u32;
    let r_variant = (variant_seed % 3) as u32;
    let c_variant = (variant_seed % 3) as u32;
    match shot.kind {
        ShotKind::TypeImpact => layout_type_impact(&mut normal, ctx.width, ctx.height),
        ShotKind::QuietTableau => layout_quiet_tableau(&mut normal, ctx.width, ctx.height, t_variant),
        ShotKind::TrackingRibbon => layout_tracking_ribbon(&mut normal, ctx.width, ctx.height, r_variant),
        ShotKind::MaskReveal => layout_cross_stack(&mut normal, ctx.width, ctx.height),
        ShotKind::EditorialColumn => layout_editorial_column(&mut normal, ctx.width, ctx.height, e_variant),
        ShotKind::FragmentCollage => layout_fragment_collage(&mut normal, ctx.width, ctx.height, ctx.seed ^ shot_idx as u64, c_variant),
        ShotKind::PosterBlocks => layout_poster_blocks(&mut normal, ctx.width, ctx.height, ctx.seed ^ shot_idx as u64),
    }
    // Giants sit behind the hero: reuse the hero's final position, slightly offset & larger.
    {
        let hero_pos = normal.iter().find(|p| p.role == Role::Hero).map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
        for (gi, g) in giants.iter_mut().enumerate() {
            g.x = hero_pos.0 - ctx.width * (0.10 - gi as f32 * 0.03);
            g.y = hero_pos.1 - ctx.height * (0.05 - gi as f32 * 0.02);
        }
    }
    // Hero floats up from below the stage (folia heroBox enter (0, height*0.15)).
    for p in &mut normal {
        if p.role == Role::Hero {
            p.enter = [0.0, ctx.height * 0.15];
        }
    }
    // Per-box 82% screen pre-fit (folia fitScale), then the global 7-step fit below.
    {
        let max_w = ctx.width * 0.82;
        let max_h = ctx.height * 0.82;
        for p in &mut normal {
            let mut fs = 1.0f32;
            if p.w > max_w {
                fs = fs.min(max_w / p.w);
            }
            if p.h > max_h {
                fs = fs.min(max_h / p.h);
            }
            if fs < 1.0 {
                p.w *= fs;
                p.h *= fs;
                p.size *= fs;
            }
        }
    }
    placements = normal;
    placements.append(&mut giants);
    // Global fit: never overflow the safe area. The safe area is already divided by the
    // shot's camera zoom (lyrics keep the full zoom + focus tracking), so the text stays
    // inside the screen while the MG background layer below uses its own weaker transform.
    let fit = fit_extent(&placements, ctx.width, ctx.height, shot.camera.zoom);

    // Shot transitions (folia resolveSonnetShotTransitionFrame). Enter fires in the first
    // `twindow` seconds of the shot; exit fires in the last `twindow` seconds *of the gap
    // leading into the next shot* — i.e. [next.start - twindow, next.start] — falling back to
    // the current shot's end when this is the final shot. Folia keys the exit window on the
    // NEXT shot's start, not the current shot's last-line end, so a wide instrumental gap
    // does not turn the screen blank for the whole gap.
    //
    // The progress argument must RISE 0→1 over the window so resolve_transition's
    // `amount = ease(progress)` ramps the alpha from 1 down to 0 (a real fade-out). The
    // previous port passed (shot.end - t)/twindow — which DECREASES 1→0 across the window —
    // inverting the curve so alpha went 0→1 *up* during the exit, hitting exactly 0 at the
    // START of the exit window and recovering to 1 at the end. That inverted ramp made the
    // whole shot disappear ~twindow before its actual end and reappear right at the cut —
    // the "转场没歌词" flash and the per-word "缺一会补上" flicker (the alpha <= 0.004 gate
    // dropped whichever words were alive in that window).
    let idle = Transition { alpha: 1.0, blur: 0.0, glitch: 0.0, pull: 0.0 };
    let enter = if t < shot.start + shot.twindow {
        resolve_transition(shot.transition, "enter", (t - shot.start) / shot.twindow)
    } else {
        idle
    };
    let exit_end = program.shots.get(shot_idx + 1).map(|s| s.start).unwrap_or(shot.end);
    let exit_start = exit_end - shot.twindow;
    let exit = if t >= exit_start && t < exit_end {
        resolve_transition(shot.transition, "exit", (t - exit_start) / shot.twindow)
    } else {
        idle
    };
    let trans_alpha = enter.alpha.min(exit.alpha);
    let trans_pull = enter.pull.max(exit.pull);
    // Paragraph transition (folia compileSonnetProgram: each paragraph carries only a
    // `transitionOut`, an exit fade covering the last `window` seconds before the next
    // paragraph starts). There is NO paragraph enter — the first shot of a paragraph
    // inherits its own shot-enter transition. The previous port added a paragraph "enter"
    // fade (alpha 0.18→1 at every paragraph start) on top, and drove the exit progress
    // BACKWARDS like the shot exit, so each paragraph boundary emptied the screen for
    // `window` seconds and then re-faded-in — double-dipping the transition with the shot
    // transition and holding the lyrics near-zero for far too long.
    let mut para_alpha = 1.0f32;
    let mut para_pull = 0.0f32;
    for (pi, (ps, pe, pkind, window)) in program.paras.iter().enumerate() {
        if pi == program.paras.len() - 1 {
            break; // folia: the final paragraph has no transitionOut
        }
        // folia: transitionOut.startTime = max(para.start, next.start - duration).
        let exit_end = *pe;
        let exit_start = (exit_end - *window).max(*ps);
        if t >= exit_start && t < exit_end && (exit_end - *ps) > 0.001 {
            let e = resolve_transition(*pkind, "exit", (t - exit_start) / window);
            para_alpha = para_alpha.min(e.alpha);
            para_pull = para_pull.max(e.pull);
        }
    }
    let trans_alpha = trans_alpha.min(para_alpha);
    let trans_pull = trans_pull.max(para_pull);
    let mut fx = LyricFx {
        blur: enter.blur.max(exit.blur),
        glitch: enter.glitch.max(exit.glitch),
        noise: ctx.post[0] * 0.35, // folia film grain: grain * 0.35
        contrast: ctx.post[1] * 0.5, // folia postProcessContrast * 0.5 (the blur addend was a port-only fake)
        glow: 0.0,
        chromatic: ctx.post[3], // folia postProcessLensDispersion: per-glyph chromatic dispersion
        rgb_shift: ctx.post[4], // folia postProcessRgbShift: full-screen RGB shift print pass
        halftone: ctx.post[5],
        vignette: ctx.post[6],
        lens_distortion: ctx.post[2], // folia postProcessLensDistortion: full-frame radial barrel
    };

    // Camera. Breath ramps in after the last glyph settles (folia resolveSonnetBreathWeight).
    let reveal_done = placements
        .iter()
        .filter(|p| !p.giant)
        .map(|p| p.end)
        .fold(shot.start, f32::max)
        .min(shot.end);
    let breath_weight = ease_in_out(((t - reveal_done) / 1.2).clamp(0.0, 1.0));
    let progress = ((ctx.time - shot.start) / (shot.end - shot.start).max(0.001)).clamp(0.0, 1.0);
    let (mut cam_scale, mut cam_px, mut cam_py, mut cam_rot) = camera_frame(shot.kind, progress, ctx.time, &shot.camera, breath_weight);
    // Gap drift: after the shot ends the camera keeps drifting along its tail direction at a
    // slow relaxed pace so the frame never freezes between shots (folia updateShot).
    if ctx.time > shot.end {
        let gap = (ctx.time - shot.end).max(0.0);
        let drift = (1.0 - (-gap * 0.4).exp()) * 2.0;
        let (s0, x0, y0, r0) = camera_frame(shot.kind, 0.8, ctx.time, &shot.camera, breath_weight);
        let (s1, x1, y1, r1) = camera_frame(shot.kind, 1.0, ctx.time, &shot.camera, breath_weight);
        cam_px += (x1 - x0) * drift;
        cam_py += (y1 - y0) * drift;
        cam_scale += (s1 - s0) * drift;
        cam_rot += (r1 - r0) * drift;
    }
    let min_d = ctx.width.min(ctx.height);

    let mut out: Vec<CharQuad> = Vec::with_capacity(160);

    // folia-style glyph settle window: 0.65–1.8s, capped at 72% of the shot.
    let shot_dur = (shot.end - shot.start).max(0.001);
    let settle = (shot_dur * 0.42).clamp(0.65, 1.8).min(shot_dur * 0.72);
    // mask-reveal: the whole composition is uncovered by a left→right wipe (folia shot mask).
    let mask_wipe = if shot.kind == ShotKind::MaskReveal {
        smooth((progress * 2.4 - 0.7).clamp(0.0, 1.0))
    } else {
        1.0
    };
    for p in &placements {
        let waiting = t < p.start;
        let fly = if waiting { 0.0 } else { ease_expo_out((t - p.start) / settle) };
        // folia coreAlpha = waiting ? 0 : 0.16 + progress*0.84 — glyphs stay visible while gliding.
        let core = if waiting { 0.0 } else { 0.16 + fly * 0.84 };
        if core <= 0.004 {
            continue;
        }
        // Accent = the word currently being sung.
        let sung = t >= p.start && t <= p.end;
        let (color, glow, pop) = if sung {
            (ctx.colors.accent, 1.0, 1.18)
        } else {
            match p.role {
                Role::Hero => (ctx.colors.accent, 0.65, 1.08),
                Role::SemiHero => (ctx.colors.primary, 0.0, 1.0),
                Role::Support => (ctx.colors.dim, 0.0, 1.0),
                Role::Decoration => ([1.0, 1.0, 1.0, if p.giant { 0.22 } else { 0.5 }], 0.0, 0.92),
            }
        };
        let alpha = trans_alpha * core * color[3] * mask_wipe;
        if alpha <= 0.004 {
            continue;
        }
        // Fly-in: offset from enter toward final; hero pops.
        let mut off = [p.enter[0] * (1.0 - fly), p.enter[1] * (1.0 - fly)];
        // folia resolveSonnetSegmentNormalOffset: support words sit ±0.3× font size along the
        // layout *normal* (rotation + π/2 for horizontal layouts; rotation for vertical). folia
        // uses a stateless per-item random; we derive ours from the segment's start-time bits
        // so it is deterministic and rebuild-stable. (The previous port only nudged Y, which
        // pinned support words to the vertical axis and broke vertical and rotated layouts.)
        if p.role == Role::Support && !p.giant {
            let seg_seed = p.start.to_bits() ^ shot.start.to_bits();
            let off_rand = sonnet_hash01(seg_seed, 0, 0x4F46) * 2.0 - 1.0; // -1..1
            let normal_angle = p.rotation + if p.vertical { 0.0 } else { std::f32::consts::FRAC_PI_2 };
            let dist = off_rand * p.size * 0.3;
            off[0] += normal_angle.cos() * dist;
            off[1] += normal_angle.sin() * dist;
        }
        // folia resolveSonnetSegmentDepth: only Decoration has depth, symmetric ±(0.5..1.3),
        // drawn from a per-segment random — half of decor sits in front (+z) and half behind
        // (−z), so the parallax pop never feels one-sided. (The previous port used
        // sin(start*7.13).abs() supporting only +z, and incorrectly gave Support a depth
        // term that folia leaves at 0.)
        let depth = if p.role == Role::Decoration {
            let seg_seed = p.start.to_bits() ^ shot.start.to_bits();
            let r0 = sonnet_hash01(seg_seed, 1, 0x4448);
            let r1 = sonnet_hash01(seg_seed, 2, 0x5040);
            if r0 > 0.5 { 0.5 + r1 * 0.8 } else { -0.5 - r1 * 0.8 }
        } else {
            0.0
        };
        if depth != 0.0 {
            off[0] -= cam_px * min_d * fit * depth * 0.9;
            off[1] -= cam_py * min_d * fit * depth * 0.9;
        }
        // Full-word entrance scale (folia: 0.86 + fly*0.14) — applied to all roles so the
        // post-transition entrance is visible. Hero keeps its extra 1.0 → 1.2 pop on top.
        let enter_scale = 0.86 + fly * 0.14;
        let hero_pop = if p.role == Role::Hero { 1.0 + (1.0 - fly) * 0.2 } else { 1.0 };
        let scale = fit * pop * enter_scale * hero_pop;
        // folia resolveSonnetRoleFontWeight: manual global override else per-role weights.
        let weight = if ctx.font_weight > 0.0 {
            let w = ctx.font_weight.round();
            if w >= 900.0 {
                2u8
            } else if w >= 700.0 {
                1u8
            } else if w >= 500.0 {
                0u8
            } else {
                3u8
            }
        } else {
            match p.role {
                Role::Hero | Role::SemiHero => 2u8, // 900
                Role::Support => 1u8,               // 700
                Role::Decoration => 3u8,            // 300 (light)
            }
        };
        // Per-glyph staggered entry (folia sonnetGlyphLayout): vertical columns stagger on
        // X by ±0.28× font size, horizontal words on Y by ±0.24×, plus a small rotation so
        // each character visibly "回正" to its line.
        let emphasis = matches!(p.role, Role::Hero | Role::SemiHero);
        let char_enter: Vec<(f32, f32, f32)> = if p.giant {
            Vec::new()
        } else {
            p.text
                .chars()
                .enumerate()
                .map(|(i, _)| {
                    let stagger = if i % 2 == 0 { -1.0 } else { 1.0 };
                    if p.vertical {
                        (stagger * p.size * 0.28, 0.0, stagger * if emphasis { 0.055 } else { 0.035 })
                    } else {
                        (0.0, stagger * p.size * 0.24, stagger * if emphasis { 0.055 } else { 0.035 })
                    }
                })
                .collect()
        };
        // Semi-hero echo ghosts: hollow copies split along the row normal, quick fade.
        let ghost = if p.role == Role::SemiHero && !p.giant {
            let gp = ((t - p.start) / 0.5).clamp(0.0, 1.0);
            if gp > 0.0 && gp < 1.0 {
                let spread = 1.0 - (1.0 - gp).powi(3);
                let env = if gp <= 0.2 { gp / 0.2 } else { (1.0 - (gp - 0.2) / 0.8).powi(2) };
                let side = if (p.start as i64) % 2 == 0 { 1.0 } else { -1.0 };
                Some((side * p.size * 0.85 * spread, 0.0, 0.22 * env))
            } else {
                None
            }
        } else {
            None
        };
        let (pen_x, pen_y) = if p.vertical {
            (p.x - p.size * 1.4 * 0.5, p.y - p.h * 0.5)
        } else {
            (p.x - p.w / 2.0, p.y + p.size * 0.35)
        };
        // Per-grapheme entry clock (folia): each character starts at its own time inside the
        // word window and settles over the same window, so words ripple in char by char.
        let chars: Vec<char> = p.text.chars().collect();
        let char_fly: Vec<f32> = if chars.len() > 1 && !p.giant {
            let dur = (p.end - p.start).max(0.08);
            chars
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let st = p.start + dur * (i as f32 / chars.len() as f32);
                    if t < st {
                        0.0
                    } else {
                        ease_expo_out((t - st) / settle)
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        // Per-glyph settle scale (folia: 0.86 + fly*0.14; type-impact emphasis 0.52 + fly*0.48).
        // Pulled in to 0.97 + fly*0.03 — the old 0.86 swing made each incoming character
        // look like an "extra" glyph at a different position; the ripple is now a subtle
        // breathing-in rather than a visible size jump.
        let char_scale: Vec<f32> = if p.giant || char_fly.is_empty() {
            Vec::new()
        } else {
            char_fly
                .iter()
                .map(|&fly_i| {
                    if emphasis && shot.kind == ShotKind::TypeImpact {
                        0.7 + fly_i * 0.3
                    } else {
                        0.97 + fly_i * 0.03
                    }
                })
                .collect()
        };
        push_word_full(
            ctx.atlas, &mut out, &p.text,
            pen_x, pen_y,
            p.size, weight, alpha, scale, p.rotation, off, color, glow,
            Some(&char_enter), 1.0 - fly,
            if emphasis { 0.04 * (1.0 - fly) } else { 0.02 * (1.0 - fly) },
            ghost, p.vertical,
            if char_fly.is_empty() { None } else { Some(&char_fly) },
            core,
            if char_scale.is_empty() { None } else { Some(&char_scale) },
        );
    }

    // folia guide: bezier lead-in curve + star head + silk threads + rect spline + shape
    // burst, drawn toward each word just before it enters (sonnetGuides.ts).
    {
        use crate::lyricview::push_circle;
        for p in &placements {
            if p.giant || p.role == Role::Decoration {
                continue;
            }
            let seg_dur = (p.end - p.start).max(0.1);
            let lead = (0.18 + seg_dur * 0.1).clamp(0.2, 0.38);
            let g_start = p.start - lead;
            let g_end = p.start + 0.65;
            if t < g_start || t > g_end {
                continue;
            }
            let gp = ((t - g_start) / (g_end - g_start)).clamp(0.0, 1.0);
            let draw_p = (gp / 0.35).min(1.0);
            let fade = 1.0 - ((gp - 0.4) / 0.3).clamp(0.0, 1.0);
            if draw_p <= 0.0 || fade <= 0.0 {
                continue;
            }
            let is_hero = matches!(p.role, Role::Hero | Role::SemiHero);
            // Deterministic per-placement randoms (folia uses Math.random at build time).
            let r1 = (p.start * 3.7).sin().abs();
            let r2 = (p.start * 8.3).sin().abs();
            let r3 = (p.start * 5.9).sin().abs();
            let r4 = (p.start * 11.7).sin().abs();
            let r5 = (p.start * 7.9).sin().abs();
            let dir_x = if p.enter[0].abs() > 0.1 { p.enter[0].signum() } else if (p.start as i64) % 2 == 0 { -1.0 } else { 1.0 };
            let font = p.size;
            let (p0x, p0y) = (dir_x * font * 1.8, -font * 0.9);
            let (p1x, p1y) = (p0x * 0.6, p0y * 0.4);
            let (p2x, p2y) = (p0x * 0.2, p0y * 0.1);
            let (p3x, p3y) = (0.0f32, 0.0f32);
            let col = if is_hero { ctx.colors.accent } else { ctx.colors.secondary };
            let base_alpha = if is_hero { 0.82 } else { 0.55 } * fade;
            let steps = 16;
            let mut prev = (p0x, p0y);
            let mut prev_i = 0usize;
            for i in 1..=steps {
                let tt = (i as f32 / steps as f32) * draw_p;
                let mt = 1.0 - tt;
                let cx = mt * mt * mt * p0x + 3.0 * mt * mt * tt * p1x + 3.0 * mt * tt * tt * p2x + tt * tt * tt * p3x;
                let cy = mt * mt * mt * p0y + 3.0 * mt * mt * tt * p1y + 3.0 * mt * tt * tt * p2y + tt * tt * tt * p3y;
                let inten = (i as f32 / steps as f32).powi(2);
                let a = base_alpha * inten.min(1.0);
                if a > 0.004 && i > prev_i {
                    crate::lyricview::push_line(&mut out, p.x + prev.0, p.y + prev.1, p.x + cx, p.y + cy, 1.2 + inten * 1.8, a, col);
                    prev = (cx, cy);
                    prev_i = i;
                }
            }
            // Glowing star head at the curve tip.
            let tt = draw_p;
            let mt = 1.0 - tt;
            let hx = mt * mt * mt * p0x + 3.0 * mt * mt * tt * p1x + 3.0 * mt * tt * tt * p2x + tt * tt * tt * p3x;
            let hy = mt * mt * mt * p0y + 3.0 * mt * mt * tt * p1y + 3.0 * mt * tt * tt * p2y + tt * tt * tt * p3y;
            let white = [1.0f32, 1.0, 1.0, 0.9 * fade];
            push_circle(&mut out, p.x + hx, p.y + hy, if is_hero { 4.5 } else { 3.0 }, fade, white);
            push_circle(&mut out, p.x + hx, p.y + hy, if is_hero { 14.0 } else { 9.0 }, 0.5 * fade, col);
            // Silk threads: probability-gated like folia (hero always, else random).
            let d = if (p.start as i64) % 2 == 0 { 1.0 } else { -1.0 };
            let yoff = (r1 - 0.5) * font * 0.8;
            let mut threads: Vec<[f32; 8]> = Vec::new();
            if is_hero || r2 > 0.4 {
                threads.push([
                    -d * font * 2.5, yoff + font * 1.5, -d * font * 0.8, yoff - font * 2.0,
                    d * font * 0.8, yoff + font * 2.0, d * font * 2.5, yoff - font * 1.5,
                ]);
            }
            if is_hero || r3 > 0.6 {
                threads.push([
                    -font * 2.0, -d * font * 1.8, font * 2.0, d * font * 1.8,
                    -font * 2.0, d * font * 1.8, font * 2.0, -d * font * 1.8,
                ]);
            }
            for (ti, th) in threads.iter().enumerate() {
                let delay = r4 * (0.15 - ti as f32 * 0.05);
                let lprog = (gp - delay) / 0.55;
                if lprog <= 0.0 || lprog >= 1.3 || fade <= 0.0 {
                    continue;
                }
                let head_t = lprog.min(1.0);
                let tail_t = (lprog - 0.35).max(0.0);
                if head_t <= tail_t {
                    continue;
                }
                let steps = 14;
                let bez = |u: f32| -> (f32, f32) {
                    let mt = 1.0 - u;
                    (
                        mt * mt * mt * th[0] + 3.0 * mt * mt * u * th[2] + 3.0 * mt * u * u * th[4] + u * u * u * th[6],
                        mt * mt * mt * th[1] + 3.0 * mt * mt * u * th[3] + 3.0 * mt * u * u * th[5] + u * u * u * th[7],
                    )
                };
                let (mut px_, mut py_) = bez(tail_t);
                for i in 1..=steps {
                    let u = tail_t + (i as f32 / steps as f32) * (head_t - tail_t);
                    let (cx_, cy_) = bez(u);
                    let inten = (i as f32 / steps as f32).powi(2);
                    let a = inten * (if is_hero { 0.82 } else { 0.55 }) * fade * 0.9;
                    let lw = if is_hero { 2.0 + inten * 5.0 } else { 1.0 + inten * 2.5 };
                    if a > 0.004 {
                        crate::lyricview::push_line(&mut out, p.x + px_, p.y + py_, p.x + cx_, p.y + cy_, lw, a, col);
                    }
                    px_ = cx_;
                    py_ = cy_;
                }
                // Silk head glow + white core + follow circle (folia trackingTrails head).
                if head_t > 0.0 && head_t < 1.0 {
                    let (hx_, hy_) = bez(head_t);
                    push_circle(&mut out, p.x + hx_, p.y + hy_, if is_hero { 7.0 } else { 4.0 }, 0.9 * fade, col);
                    push_circle(&mut out, p.x + hx_, p.y + hy_, if is_hero { 2.5 } else { 1.5 }, fade, white);
                    push_circle(&mut out, p.x + hx_, p.y + hy_, if is_hero { 20.0 } else { 12.0 }, 0.4 * fade, col);
                }
            }
            // Rect spline: a growing HUD-style bar at a random angle (folia rectSpline, 60%).
            if r4 > 0.4 {
                let length = font * (1.2 + r5 * 1.5);
                let thickness = (if is_hero { 6.0 } else { 3.0 }) + r1 * 8.0;
                let angle = (r2 - 0.5) * std::f32::consts::PI;
                let rx = (r3 - 0.5) * font * 1.2;
                let ry = (r1 - 0.5) * font * 1.2;
                let dur = 0.25 + r5 * 0.2;
                let lprog = (gp - r4 * 0.15) / dur;
                if lprog > 0.0 && lprog < 1.3 && fade > 0.0 {
                    let head = (lprog * 1.5).min(1.0);
                    let tail = ((lprog - 0.3) * 1.5).min(1.0).max(0.0);
                    if head > tail {
                        let (ax, ay) = (rx + angle.cos() * length * tail, ry + angle.sin() * length * tail);
                        let (bx, by) = (rx + angle.cos() * length * head, ry + angle.sin() * length * head);
                        crate::lyricview::push_line(&mut out, p.x + ax, p.y + ay, p.x + bx, p.y + by, thickness, 0.5 * fade, col);
                        crate::lyricview::push_line(&mut out, p.x + ax, p.y + ay, p.x + bx, p.y + by, thickness * 0.3, 0.7 * fade, white);
                    }
                }
            }
            // Shape burst: small geometric particles exploding outward after progress 0.3.
            let burst = ((gp - 0.3) / 0.7).clamp(0.0, 1.0);
            if burst > 0.0 && fade > 0.0 {
                let n = if is_hero { 4 } else { 2 };
                for k in 0..n {
                    let sk = (p.start * (17.0 + k as f32 * 3.0)).sin().abs();
                    let ang = (p.start * (23.0 + k as f32 * 5.0)).sin().abs() * std::f32::consts::TAU;
                    let spd = (15.0 + sk * 45.0) * if is_hero { 1.0 } else { 0.7 };
                    let ease = 1.0 - (1.0 - burst).powi(3);
                    let sx = ang.cos() * spd * ease;
                    let sy = ang.sin() * spd * ease;
                    let ssize = ((if is_hero { 3.0 } else { 1.5 }) + sk * 3.5) * (1.0 - burst * 0.4);
                    let salpha = (1.0 - burst) * if is_hero { 0.8 } else { 0.6 } * fade;
                    let srot = (sk - 0.5) * 8.0 * burst;
                    let stype = (k * 7) % 4;
                    match stype {
                        0 => push_circle(&mut out, p.x + sx, p.y + sy, ssize, salpha, col),
                        1 => {
                            crate::lyricview::push_rect(&mut out, p.x + sx, p.y + sy, ssize * 2.0, ssize * 2.0, salpha, col);
                        }
                        2 => {
                            crate::lyricview::push_line(&mut out, p.x + sx - ssize, p.y + sy, p.x + sx + ssize, p.y + sy, 2.0, salpha, col);
                            crate::lyricview::push_line(&mut out, p.x + sx, p.y + sy - ssize, p.x + sx, p.y + sy + ssize, 2.0, salpha, col);
                        }
                        _ => {
                            let s2 = ssize * 0.8;
                            crate::lyricview::push_line(&mut out, p.x + sx, p.y + sy - s2, p.x + sx + s2, p.y + sy, 1.5, salpha, col);
                            crate::lyricview::push_line(&mut out, p.x + sx + s2, p.y + sy, p.x + sx, p.y + sy + s2, 1.5, salpha, col);
                            crate::lyricview::push_line(&mut out, p.x + sx, p.y + sy + s2, p.x + sx - s2, p.y + sy, 1.5, salpha, col);
                            crate::lyricview::push_line(&mut out, p.x + sx - s2, p.y + sy, p.x + sx, p.y + sy - s2, 1.5, salpha, col);
                        }
                    }
                }
            }
        }
    }

    // Camera + fit applied to the whole shot (positions are in stage-local coords). The
    // sung-word focus pulls the frame toward the text being sung (smoothly, half strength).
    let cam_scale_f = cam_scale * fit * (1.0 - trans_pull * 0.16);
    let (focus_x, focus_y) = resolved_focus(ctx, &placements, t);
    if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
        let sung = placements.iter().find(|p| !p.giant && t >= p.start && t <= p.end);
        eprintln!("DBG t={t:.2} cam_scale_f={cam_scale_f:.3} pan=({cam_px:.3},{cam_py:.3}) fit={fit:.3} focus=({focus_x:.0},{focus_y:.0}) sung={:?}", sung.map(|p| (p.text.clone(), p.x as i32, p.y as i32)));
    }
    // folia: pivot = basePivot + (focus - basePivot) * cameraIntensity (default 1.0).
    let focus_track = 1.0;
    let cam_px_final = cam_px * min_d * fit - focus_x * cam_scale_f * focus_track + trans_pull * min_d * 0.05;
    let cam_py_final = cam_py * min_d * fit - focus_y * cam_scale_f * focus_track - trans_pull * min_d * 0.03;
    let _t_cam_start = std::time::Instant::now();
    apply_camera_local(
        &mut out,
        ctx.width * 0.5,
        ctx.height * 0.5,
        cam_scale_f,
        cam_px_final,
        cam_py_final,
        cam_rot,
    );
    _t_cam = _t_cam_start.elapsed();

    // Motion-graphics decorative background (folia's sonnetShotMg): HUD / geometric chaos /
    // fixed geometry / particles / scanlines. Built per shot and emitted with the same camera
    // so the decoration moves with the lyrics. All procedural — no images.
    {
        let mg_kind = match shot.kind {
            ShotKind::EditorialColumn => crate::lyricstyles::mg_scene::MgShotKind::EditorialColumn,
            ShotKind::TypeImpact => crate::lyricstyles::mg_scene::MgShotKind::TypeImpact,
            ShotKind::FragmentCollage => crate::lyricstyles::mg_scene::MgShotKind::FragmentCollage,
            ShotKind::TrackingRibbon => crate::lyricstyles::mg_scene::MgShotKind::TrackingRibbon,
            ShotKind::MaskReveal => crate::lyricstyles::mg_scene::MgShotKind::MaskReveal,
            ShotKind::PosterBlocks => crate::lyricstyles::mg_scene::MgShotKind::PosterBlocks,
            ShotKind::QuietTableau => crate::lyricstyles::mg_scene::MgShotKind::QuietTableau,
        };
        let mk = |c: [f32; 4]| [c[0], c[1], c[2], 1.0];
        let last_word = placements.last().map(|p| p.start).unwrap_or(shot.start);
        let target_finish = last_word
            .max(shot.start + (shot.end - shot.start) * 0.95)
            .min(shot.end);
        let draw_duration = (target_finish - shot.start).max(1.0);
        let raw_progress = ((t - shot.start) / draw_duration).clamp(0.0, 1.0);
        let cam = crate::lyricstyles::mg_scene::MgCam {
            // Folia keeps the MG background on a weaker transform than the lyrics: the
            // particle/parallax layer scales by only 30% of the camera push and pans at
            // 40%, so the decoration reads as a distant layer instead of being zoomed
            // off-screen along with the text.
            zoom: 1.0 + (cam_scale_f - 1.0) * 0.3,
            px: cam_px_final * 0.4,
            py: cam_py_final * 0.4,
            rot: cam_rot,
            cx: ctx.width * 0.5,
            cy: ctx.height * 0.5,
        };
        let cache_key = (ctx.seed, shot_idx);
        let mut mg_out: Vec<CharQuad> = Vec::new();
        let _t_mg_start = std::time::Instant::now();
        MG_CACHE.with(|c| {
            let mut map = c.borrow_mut();
            if map.len() > 3 {
                map.clear();
            }
            let scene = map.entry(cache_key).or_insert_with(|| {
                crate::lyricstyles::mg_scene::build_shot_mg(
                    mg_kind,
                    ctx.width,
                    ctx.height,
                    (ctx.seed ^ shot_idx as u64) as i64,
                    mk(ctx.colors.primary),
                    mk(ctx.colors.secondary),
                    mk(ctx.colors.accent),
                    ctx.mg_bg,
                    ctx.mg_fixed,
                    ctx.mg_decor,
                )
            });
            scene.emit(raw_progress, t, shot.start, shot.end, ctx.audio, &cam, &mut mg_out);
        });
        _t_mg = _t_mg_start.elapsed();
        // Prepend MG decorations so the QUAD_BUDGET truncation drops the LYRIC tail,
        // not the MG head. Without this, scanlines / HUD / geometric chaos are the first
        // to be sliced off whenever a busy line produces >~200 lyric quads (which is
        // almost always — a 12-char hero + 2 giant echoes + 5 supports ≈ 60 quads alone).
        let mut combined = mg_out;
        combined.append(&mut out);
        const QUAD_BUDGET: usize = 1024;
        if combined.len() > QUAD_BUDGET {
            combined.truncate(QUAD_BUDGET);
        }
        out = combined;
    }

    // Scene metadata text: vertical "[ SONNET ]" tag at the left edge (folia scene builder).
    {
        let tag = "[ SONNET ]";
        let tsize = ctx.height * 0.022;
        let tw = measure_text(ctx.atlas, tag, tsize);
        push_word_full(
            ctx.atlas, &mut out, tag,
            -ctx.width * 0.46, ctx.height * 0.48 - tw * 0.5,
            tsize, 0, 0.22, 1.0, -std::f32::consts::FRAC_PI_2, [0.0, 0.0], ctx.colors.primary, 0.0,
            None, 0.0, 0.0, None, false, None, 0.0, None,
        );
    }

    // Decorative open frames around a deterministic subset of hero words: thin lines tracing
    // in from the four corners (no chunky corner boxes).
    let mut rng = Seeded::new(ctx.seed.wrapping_add(shot_idx as u64 * 0x77));
    for p in &placements {
        if p.role != Role::Hero {
            continue;
        }
        if rng.unit() > 0.6 {
            continue;
        }
        let fly = ease_expo_out((t - p.start) / 0.45);
        if fly < 0.7 {
            continue;
        }
        let grow = smooth((fly - 0.7) / 0.35);
        let half_w = p.w / 2.0 * fit;
        let half_h = p.h / 2.0 * fit;
        let bar = 2.0;
        let len = (14.0 + 12.0 * grow) * (0.7 + 0.3 * grow);
        let inset = 6.0;
        let col = ctx.colors.primary;
        let a = trans_alpha * 0.55 * grow;
        if a <= 0.004 {
            continue;
        }
        // Top-left corner: vertical + horizontal lines growing inward.
        push_rect(&mut out, p.x - half_w - inset, p.y - half_h - inset - len / 2.0, bar, len, a, col);
        push_rect(&mut out, p.x - half_w - inset - len / 2.0, p.y - half_h - inset, len, bar, a, col);
        // Top-right corner.
        push_rect(&mut out, p.x + half_w + inset, p.y - half_h - inset - len / 2.0, bar, len, a, col);
        push_rect(&mut out, p.x + half_w + inset + len / 2.0, p.y - half_h - inset, len, bar, a, col);
        // Bottom-left corner.
        push_rect(&mut out, p.x - half_w - inset, p.y + half_h + inset + len / 2.0, bar, len, a, col);
        push_rect(&mut out, p.x - half_w - inset - len / 2.0, p.y + half_h + inset, len, bar, a, col);
        // Bottom-right corner.
        push_rect(&mut out, p.x + half_w + inset, p.y + half_h + inset + len / 2.0, bar, len, a, col);
        push_rect(&mut out, p.x + half_w + inset + len / 2.0, p.y + half_h + inset, len, bar, a, col);
    }

    // Translation subtitle (plain text, no background pill).
    if !input.translation.is_empty() {
        let current_line = shot.line_range.clone().find(|&li| lines[li].start_ms as f32 / 1000.0 <= t)
            .unwrap_or(shot.line_range.start);
        let timing = LineTiming {
            start: lines[current_line].start_ms as f32 / 1000.0,
            end: line_end(&lines[current_line], lines.get(current_line + 1)),
            duration: 1.0,
        };
        let fade = 0.25f32;
        let a_in = ((t - timing.start) / fade).clamp(0.0, 1.0);
        let a_out = ((timing.end - t) / fade).clamp(0.0, 1.0);
        // Never fully hidden by shot transitions: clamp to a comfortable floor.
        let t_alpha = (0.95 * a_in.min(a_out)).max(0.35);
        if t_alpha > 0.004 {
            let size = scales.subtitle;
            let text_w = measure_text(ctx.atlas, input.translation, size);
            let bar_y = ctx.height * 0.90;
            push_word_full(
                ctx.atlas, &mut out, input.translation,
                ctx.width * 0.5 - text_w * 0.5, bar_y + size * 0.35,
                size, 1, t_alpha, 1.0, 0.0, [0.0, 0.0], ctx.colors.primary, 0.0,
                None, 0.0, 0.0, None, false, None, 0.0, None,
            );
        }
    }

    // End-of-song credits poster (folia sonnetCredits): after the final line, fade in
    // title/artist/album and blur the outgoing scene.
    let mut outro_blur = 0.0f32;
    if let Some(last) = lines.last() {
        let song_end = line_end(last, None);
        if t > song_end + 0.4 {
            let cprog = ((t - song_end - 0.4) / 1.6).clamp(0.0, 1.0);
            let cease = ease_in_out(cprog);
            outro_blur = (0.3 + cease * 0.9) * 0.5;
            if !input.song_title.is_empty() {
                let a = cease * 0.95;
                let size = scales.main * 1.35;
                let title_w = measure_text_bold(ctx.atlas, input.song_title, size);
                let ty = ctx.height * 0.58;
                push_word_full(
                    ctx.atlas, &mut out, input.song_title,
                    ctx.width * 0.5 - title_w * 0.5, ty,
                    size, 2, a, 1.0, 0.0, [0.0, 0.0], ctx.colors.primary, 0.15,
                    None, 0.0, 0.0, None, false, None, 0.0, None,
                );
                if !input.song_artist.is_empty() {
                    let asize = scales.main * 0.5;
                    let aw = measure_text(ctx.atlas, input.song_artist, asize);
                    push_word_full(
                        ctx.atlas, &mut out, input.song_artist,
                        ctx.width * 0.5 - aw * 0.5, ty + size * 1.1,
                        asize, 1, a * 0.85, 1.0, 0.0, [0.0, 0.0], ctx.colors.secondary, 0.0,
                        None, 0.0, 0.0, None, false, None, 0.0, None,
                    );
                }
            }
        }
    }
    fx.blur = fx.blur.max(outro_blur);

    if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
        let _t_total = _t_total.elapsed();
        eprintln!("PERF total={:.2}ms compile={:.2}ms placements={:.2}ms cam={:.2}ms mg={:.2}ms n={}",
            _t_total.as_secs_f64()*1000.0,
            _t_compile.as_secs_f64()*1000.0,
            _t_placements.as_secs_f64()*1000.0,
            _t_cam.as_secs_f64()*1000.0,
            _t_mg.as_secs_f64()*1000.0,
            out.len());
    }

    // ---- folia drawOverlay: asymmetrical perimeter accents + floating cross/diamond/star.
    // Drawn after lyrics + MG so they sit on top. Uses push_rect / push_line (SLOT_FRAME)
    // which the shader renders as a low-corner-radius filled rect / line — no SDF needed.
    {
        use crate::lyricview::{push_circle, push_line, push_rect};
        let w = ctx.width;
        let h = ctx.height;
        let pad_x = (30.0_f32).max(w * 0.05);
        let pad_y = (30.0_f32).max(h * 0.05);
        let primary = ctx.colors.primary;
        let a_dim = 0.5_f32;
        let a_bright = 0.8_f32;
        let a_diamond = 0.7_f32;
        // 1. Top-Left cluster: thick bar + dropping vertical line.
        push_rect(&mut out, pad_x + 15.0, pad_y + 2.0, 30.0, 4.0, a_bright, primary);
        push_line(&mut out, pad_x, pad_y + 16.0, pad_x, pad_y + 120.0, 1.0, a_dim, primary);
        // 2. Bottom-Right cluster: thick vertical bar + horizontal line + rising line.
        push_rect(&mut out, w - pad_x - 2.0, h - pad_y - 8.0, 4.0, 16.0, a_bright, primary);
        push_line(&mut out, w - pad_x - 160.0, h - pad_y, w - pad_x - 20.0, h - pad_y, 1.0, a_dim, primary);
        push_line(&mut out, w - pad_x, h - pad_y - 180.0, w - pad_x, h - pad_y - 30.0, 1.0, a_dim, primary);
        // 3. Top-Right cross-hair (6px arms).
        let cx = w - pad_x;
        let cy = pad_y + 20.0;
        push_line(&mut out, cx - 6.0, cy, cx + 6.0, cy, 1.0, a_bright, primary);
        push_line(&mut out, cx, cy - 6.0, cx, cy + 6.0, 1.0, a_bright, primary);
        // 4. Bottom-Left diamond (4px, filled). Approximated with a 45°-rotated thin rect.
        let dx = pad_x;
        let dy = h - pad_y;
        push_rect(&mut out, dx, dy, 4.0, 4.0, a_diamond, primary);
        // 5. Typographic star ✦ at bottom-right: render as a small filled circle with a
        // brighter ring (folia uses the U+2726 glyph; a 3px dot reads the same visually
        // and avoids rasterising yet another unicode codepoint into the SDF atlas).
        let sx = w - pad_x - 10.0;
        let sy = h - pad_y;
        push_circle(&mut out, sx, sy, 3.0, 0.6, primary);
    }

    StyleOutput { quads: out, fx }
}
