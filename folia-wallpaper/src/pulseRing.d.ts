// Types for the pulseRing bridge exposed by electron-wallpaper/preload.js.
// Audio (128 bands + energy) + lyrics + playback clock + theme + config.

export interface PulseRingAudio {
  bands: Float32Array;   // 128 FFT bins (0..1)
  energy: number;        // overall energy 0..1
  bass: number;          // peak of bins 0..8
  mid: number;           // peak of bins 8..96
  treble: number;        // peak of bins 96..128
  timestamp: number;     // Date.now()
}

export interface PulseRingLyricWord {
  startTime: number;
  endTime: number;
  text: string;
}
export interface PulseRingLyricLine {
  startTime: number;
  endTime: number;
  fullText: string;
  words: PulseRingLyricWord[];
  translation?: string;
  isChorus?: boolean;
}
export interface PulseRingLyricData {
  lines: PulseRingLyricLine[];
  offset?: number;
}

export interface PulseRingPlayback {
  positionSec: number;
  durationSec: number;
  playing: boolean;
  title: string;
  artist: string;
  album?: string;
  coverUrl?: string | null;
  seed?: string;
}

export interface PulseRingConfig {
  // The visualizer mode to render (e.g. 'sonnet', 'monet', 'classic'). Driven by
  // project.json `params.visualizerMode` when the folia-lyrics pack is resolved.
  visualizerMode?: string;
  // Per-mode tuning overrides (VisualizerTuningBundle) injected by pulse-ring from
  // the user's ~/.config/pulse-ring/folia-lyrics.json. Each present mode's tuning
  // is shallow-merged over folia's DEFAULT_*_TUNING by applyVisualizerTuning.
  foliaTuning?: Record<string, unknown>;
  // Any other manifest params are passed through untouched.
  [key: string]: unknown;
}

// A minimal folia Theme shape (the fields visualizers actually read).
export interface PulseRingTheme {
  name?: string;
  backgroundColor: string;
  primaryColor: string;
  accentColor: string;
  secondaryColor: string;
  fontStyle?: 'sans' | 'serif' | 'mono';
  fontFamily?: string;
  fontFamilyStack?: string[];
  fontWeight?: number;
  animationIntensity?: 'calm' | 'normal' | 'chaotic';
  wordColors?: { word: string; color: string }[];
  lyricsIcons?: string[];
}

export interface PulseRingApi {
  apiVersion: number;
  onAudio: (cb: (d: PulseRingAudio) => void) => () => void;
  onBands: (cb: (d: PulseRingAudio) => void) => () => void;
  getAudioData: () => PulseRingAudio | null;
  onConfig: (cb: (cfg: PulseRingConfig) => void) => () => void;
  getConfig: () => PulseRingConfig | null;
  onLyrics: (cb: (d: PulseRingLyricData | null) => void) => () => void;
  onPlayback: (cb: (d: PulseRingPlayback | null) => void) => () => void;
  onTheme: (cb: (d: PulseRingTheme | null) => void) => () => void;
  getLyricData: () => PulseRingLyricData | null;
  getPlaybackState: () => PulseRingPlayback | null;
  getTheme: () => PulseRingTheme | null;
}

interface Window {
  pulseRing?: PulseRingApi;
}
