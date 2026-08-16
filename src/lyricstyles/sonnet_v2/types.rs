//! Public, renderer-independent contracts for the Sonnet PV program.
//!
//! Byte-identical 1:1 port of folia `src/types.ts` (`Line`/`Word`/`Syllable`) and
//! `src/components/visualizer/sonnet/types.ts` (`GraphemeTiming` actually lives in
//! `utils/lyrics/graphemeTiming.ts`).
//!
//! # Time units
//! folia `Line`/`Word`/`Syllable` declare `startTime`/`endTime` as JS `number`
//! (seconds). The unified Rust ingest layer (`crate::lyrics::LyricLine`) keeps
//! times in ms (`start_ms`/`end_ms`) — those are NOT the same contract. Phase 4+
//! (the program builder boundary) is responsible for the ms → seconds conversion;
//! the pure-algorithm sonnet layer here mirrors folia's seconds contract 1:1 so
//! the TS → Rust transcription stays byte-faithful.
//!
//! The full `SonnetParagraph`/`SonnetShot`/`SonnetShotKind`/`SonnetProgram`/
//! `SonnetSegment`/`SonnetAnimationCue`/… declarations land in Phase 3.9. The
//! minimal subset here covers what `grapheme_timing` + `render_hints` consume today
//! (the `LineRenderHints` family is declared here because it is part of the public
//! `Line` contract — folia `types.ts` imports it from `renderHints.ts` and exposes
//! `Line.renderHints?: LineRenderHints`).

/// folia `utils/lyrics/graphemeTiming.ts` — `GraphemeTiming`.
///
/// `char` is a single extended grapheme cluster (not necessarily a Rust `char`).
/// Times in seconds, mirroring the folia `number` contract 1:1.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphemeTiming {
    pub char: String,
    pub start_time: f64,
    pub end_time: f64,
    /// Present when the timing was derived from a `Word` index; `None` for
    /// gap-fill / unmodelled graphemes (matches the TS `wordIndex?: number`
    /// optionality — `undefined` in JS ↔ `None` in Rust).
    pub word_index: Option<usize>,
}

/// folia `types.ts` — `Syllable`. Minimal subset consumed by `grapheme_timing`.
/// Full interface (ruby / endsWithSpace / romanisedText) lands in Phase 3.9.
#[derive(Debug, Clone)]
pub struct Syllable {
    pub text: String,
    /// Seconds.
    pub start_time: f64,
    /// Seconds (>= start_time).
    pub end_time: f64,
}

impl Syllable {
    pub fn start_sec(&self) -> f64 {
        self.start_time
    }
    pub fn end_sec(&self) -> f64 {
        self.end_time
    }
}

/// folia `types.ts` — `Word`.
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    /// Seconds.
    pub start_time: f64,
    /// Seconds (>= start_time).
    pub end_time: f64,
    /// Optional sub-word syllable timing (empty when the source only provides
    /// word-level). Maps to TS `syllables?: Syllable[]`.
    pub syllables: Vec<Syllable>,
}

impl Word {
    pub fn start_sec(&self) -> f64 {
        self.start_time
    }
    pub fn end_sec(&self) -> f64 {
        self.end_time
    }
}

// ===== folia `renderHints.ts` — `LineTimingClass` / `LineTransitionMode` / `WordRevealMode` =====
// Declared here (not in `lyrics_util::render_hints`) because they are part of the
// public `Line` contract via `Line.renderHints?: LineRenderHints`.

/// folia `renderHints.ts` — `LineTimingClass = 'normal' | 'short' | 'micro'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTimingClass {
    Normal,
    Short,
    Micro,
}

/// folia `renderHints.ts` — `LineTransitionMode = 'normal' | 'fast' | 'none'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTransitionMode {
    Normal,
    Fast,
    None,
}

/// folia `renderHints.ts` — `WordRevealMode = 'normal' | 'fast' | 'instant'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordRevealMode {
    Normal,
    Fast,
    Instant,
}

/// folia `renderHints.ts` — `LineRenderHints`.
///
/// Times are in seconds. `render_end_time` is the latest point a visualizer may keep
/// this line on screen for active/pass/exit polish after reveal finishes — it is
/// NOT a guaranteed standalone timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineRenderHints {
    pub raw_duration: f64,
    pub timing_class: LineTimingClass,
    pub render_end_time: f64,
    pub line_transition_mode: LineTransitionMode,
    pub word_reveal_mode: WordRevealMode,
}

/// folia `renderHints.ts` — `LineTransitionTiming`.
///
/// Times are in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineTransitionTiming {
    pub enter_duration: f64,
    pub exit_duration: f64,
    pub line_pass_hold: f64,
}

/// folia `types.ts` — `Line`. Minimal subset consumed by `grapheme_timing` /
/// `render_hints`. Full field set (translation / id / agentId / romanisation /
/// alternateTexts / backgroundVocals / chorusEffect) lands in Phase 3.9 alongside
/// `SonnetProgram`.
#[derive(Debug, Clone)]
pub struct Line {
    pub words: Vec<Word>,
    /// Seconds.
    pub start_time: f64,
    /// Seconds (>= start_time).
    pub end_time: f64,
    /// The string shown to the user; may include whitespace / punctuation not
    /// present in `words[]`.
    pub full_text: String,
    /// Cached render hints. The pure-algorithm port calls `build_line_render_hints`
    /// lazily when this is `None`; the migrate/ensure family populates it for later
    /// visualizers that read `line.renderHints` directly.
    pub render_hints: Option<LineRenderHints>,
}

impl Line {
    pub fn start_sec(&self) -> f64 {
        self.start_time
    }
    pub fn end_sec(&self) -> f64 {
        self.end_time
    }
}

// ===== folia `sonnet/types.ts` — SonnetProgram family =====
// Pure renderer-independent contracts for the deterministic Sonnet PV program.

/// folia `sonnet/types.ts` — `SonnetParagraphKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetParagraphKind {
    Breath,
    Verse,
    Lift,
    Chorus,
    Break,
    Outro,
}

/// folia `sonnet/types.ts` — `SonnetParagraphBoundary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetParagraphBoundary {
    SongStart,
    TimeGap,
    Metadata,
    DurationCap,
    LineCap,
}

/// folia `sonnet/types.ts` — `SonnetShotKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetShotKind {
    EditorialColumn,
    TypeImpact,
    FragmentCollage,
    TrackingRibbon,
    MaskReveal,
    PosterBlocks,
    QuietTableau,
}

/// folia `sonnet/types.ts` — `SONNET_TRANSITION_KINDS` (order matters for deterministic round-robin).
pub const SONNET_TRANSITION_KINDS: &[SonnetTransitionKind] = &[
    SonnetTransitionKind::FastBlur,
    SonnetTransitionKind::MonoGlitch,
    SonnetTransitionKind::CameraPull,
];

/// folia `sonnet/types.ts` — `SonnetTransitionKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetTransitionKind {
    FastBlur,
    MonoGlitch,
    CameraPull,
}

/// folia `sonnet/types.ts` — `SonnetSemanticSegment`.
#[derive(Debug, Clone)]
pub struct SonnetSemanticSegment {
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub word_indices: Vec<usize>,
    pub graphemes: Vec<GraphemeTiming>,
    pub is_word_like: bool,
}

/// folia `sonnet/types.ts` — `SonnetAnimationCue`. `kind` is one of
/// `'enter' | 'hold' | 'exit' | 'accent'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonnetAnimationCueKind {
    Enter,
    Hold,
    Exit,
    Accent,
}

/// folia `sonnet/types.ts` — `SonnetAnimationCue`.
#[derive(Debug, Clone)]
pub struct SonnetAnimationCue {
    pub at: f64,
    pub duration: f64,
    pub kind: SonnetAnimationCueKind,
    pub segment_start: usize,
    pub segment_end: usize,
}

/// folia `sonnet/types.ts` — `SonnetCameraFrame`
#[derive(Debug, Clone, Copy)]
pub struct SonnetCameraFrame {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub rotation: f64,
}

/// folia `sonnet/types.ts` — `SonnetShot`.
#[derive(Debug, Clone)]
pub struct SonnetShot {
    pub id: String,
    pub kind: SonnetShotKind,
    pub start_time: f64,
    pub end_time: f64,
    pub line_indices: Vec<usize>,
    pub cues: Vec<SonnetAnimationCue>,
    pub camera: SonnetCameraFrame,
}

/// folia `sonnet/types.ts` — `SonnetTransition`.
#[derive(Debug, Clone)]
pub struct SonnetTransition {
    pub kind: SonnetTransitionKind,
    pub start_time: f64,
    pub end_time: f64,
}

/// folia `sonnet/types.ts` — `SonnetCompiledLine`.
#[derive(Debug, Clone)]
pub struct SonnetCompiledLine {
    pub source_index: usize,
    pub line: Line,
    pub render_end_time: f64,
    pub segments: Vec<SonnetSemanticSegment>,
}

/// folia `sonnet/types.ts` — `SonnetParagraph`.
#[derive(Debug, Clone)]
pub struct SonnetParagraph {
    pub id: String,
    pub kind: SonnetParagraphKind,
    pub boundary: SonnetParagraphBoundary,
    pub start_time: f64,
    pub end_time: f64,
    pub lines: Vec<SonnetCompiledLine>,
    pub shots: Vec<SonnetShot>,
    pub transition_out: Option<SonnetTransition>,
}

/// folia `sonnet/types.ts` — `SonnetProgram`.
#[derive(Debug, Clone)]
pub struct SonnetProgram {
    pub version: u32,
    pub seed: String,
    pub paragraph_gap_threshold: f64,
    pub paragraphs: Vec<SonnetParagraph>,
}
