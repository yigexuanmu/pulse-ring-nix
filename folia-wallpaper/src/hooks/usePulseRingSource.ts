import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  LyricData,
  Line,
  Theme,
} from '../types';
import type {
  PulseRingLyricData,
  PulseRingPlayback,
  PulseRingTheme,
} from '../pulseRing.d';
import { currentWebLyricTimeSec } from '../utils/webLyricSource';
import { migrateLyricLinesRenderHints } from '../utils/lyrics/renderHints';
import type { WebLyricClock } from '../types/webLyricSource';

// Convert pulse-ring's flattened lyric JSON into folia's LyricData (with renderHints).
const toLyricData = (raw: PulseRingLyricData | null): LyricData | null => {
  if (!raw || !raw.lines || raw.lines.length === 0) return null;
  const lines: Line[] = raw.lines.map(l => ({
    startTime: l.startTime,
    endTime: l.endTime,
    fullText: l.fullText,
    words: (l.words || []).map(w => ({ startTime: w.startTime, endTime: w.endTime, text: w.text })),
    translation: l.translation,
    isChorus: l.isChorus,
    backgroundVocals: [],
  }));
  const { value } = migrateLyricLinesRenderHints(lines);
  return { lines: value };
};

const toTheme = (raw: PulseRingTheme | null, fallback: Theme): Theme => {
  if (!raw) return fallback;
  return {
    name: raw.name || 'pulse-ring',
    backgroundColor: raw.backgroundColor ?? fallback.backgroundColor,
    primaryColor: raw.primaryColor ?? fallback.primaryColor,
    accentColor: raw.accentColor ?? fallback.accentColor,
    secondaryColor: raw.secondaryColor ?? fallback.secondaryColor,
    fontStyle: raw.fontStyle ?? 'sans',
    fontFamily: raw.fontFamily,
    fontFamilyStack: raw.fontFamilyStack,
    fontWeight: raw.fontWeight,
    animationIntensity: raw.animationIntensity ?? 'normal',
    wordColors: raw.wordColors,
    lyricsIcons: raw.lyricsIcons,
  };
};

export interface PulseRingSourceState {
  lyrics: LyricData | null;
  clock: WebLyricClock;
  track: { name: string; artist: string; coverUrl: string | null; seed?: string } | null;
}

const DEFAULT_FALLBACK_THEME: Theme = {
  name: 'pulse-ring',
  backgroundColor: '#060512',
  primaryColor: '#EADDFF',
  accentColor: '#FFD740',
  secondaryColor: '#B8B4C8',
  fontStyle: 'sans',
  animationIntensity: 'normal',
};

export interface UsePulseRingSourceResult {
  state: PulseRingSourceState;
  theme: Theme;
  getCurrentTimeSec: (nowMs: number) => number;
}

export function usePulseRingSource(): UsePulseRingSourceResult {
  const api = typeof window !== 'undefined' ? window.pulseRing : undefined;
  const [state, setState] = useState<PulseRingSourceState>({
    lyrics: null,
    clock: { positionSec: 0, durationSec: 0, anchoredAtMs: 0, playing: false },
    track: null,
  });
  const [theme, setTheme] = useState<Theme>(DEFAULT_FALLBACK_THEME);
  const clockRef = useRef(state.clock);
  clockRef.current = state.clock;

  // lyrics
  useEffect(() => {
    if (!api?.onLyrics) return;
    const off = api.onLyrics(d => setState(s => ({ ...s, lyrics: toLyricData(d) })));
    return off;
  }, [api]);

  // playback (clock + track)
  useEffect(() => {
    if (!api?.onPlayback) return;
    const off = api.onPlayback(d => {
      if (!d) { setState(s => ({ ...s, clock: { ...s.clock, playing: false }, track: null })); return; }
      setState(s => ({
        ...s,
        clock: {
          positionSec: Math.max(0, d.positionSec),
          durationSec: d.durationSec || 0,
          anchoredAtMs: Date.now(),
          playing: !!d.playing,
        },
        track: { name: d.title || '', artist: d.artist || '', coverUrl: d.coverUrl ?? null, seed: d.seed || d.title },
      }));
    });
    return off;
  }, [api]);

  // theme
  useEffect(() => {
    if (!api?.onTheme) return;
    const off = api.onTheme(d => setTheme(toTheme(d, DEFAULT_FALLBACK_THEME)));
    return off;
  }, [api]);

  const getCurrentTimeSec = useCallback((nowMs: number) => currentWebLyricTimeSec(clockRef.current, nowMs), []);

  // Seed initial data if present (preload may have buffered it before subscribe).
  useEffect(() => {
    if (!api) return;
    if (api.getLyricData?.()) setState(s => ({ ...s, lyrics: toLyricData(api.getLyricData()!) }));
    if (api.getPlaybackState?.()) {
      const p = api.getPlaybackState()!;
      setState(s => ({
        ...s,
        clock: { positionSec: Math.max(0, p.positionSec), durationSec: p.durationSec || 0, anchoredAtMs: Date.now(), playing: !!p.playing },
        track: { name: p.title || '', artist: p.artist || '', coverUrl: p.coverUrl ?? null, seed: p.seed || p.title },
      }));
    }
    if (api.getTheme?.()) setTheme(toTheme(api.getTheme()!, DEFAULT_FALLBACK_THEME));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { state, theme, getCurrentTimeSec };
}
