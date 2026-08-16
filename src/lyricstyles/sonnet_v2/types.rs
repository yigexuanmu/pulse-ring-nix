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
