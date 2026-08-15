import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useMotionValue } from 'framer-motion';
import VisualizerRenderer from './components/visualizer/VisualizerRenderer';
import { buildVisualizerTheme } from './components/app/presentation/buildVisualizerTheme';
import { findLatestActiveLineIndex } from './utils/appPlaybackHelpers';
import type { Line, Theme } from './types';
import type { VisualizerBackgroundConfig } from './components/visualizer/backgrounds/definition';
import type { VisualizerMode } from './types';
import { usePulseRingSource } from './hooks/usePulseRingSource';

// pulse-ring folia wallpaper shell.
//
// Mirrors folia's ObsWebSourceApp but driven by pulse-ring's Electron bridge
// (window.pulseRing) instead of a WS WebLyricSource:
//   - lyrics/track: from MPRIS, Rust-pushed as resolved LyricData (no in-page parser)
//   - clock: extrapolated client-side (currentWebLyricTimeSec) from playback events
//   - audio: 128-band FFT + energy (unlike pure OBS source, pulse-ring HAS audio)
//   - theme: Rust-pushed from config.qml theme palette
// Renders transparent over the wallpaper; the wgpu ring draws above it.

const SPECTRUM_BINS = 128;
const REUSE_SPECTRUM = new Uint8Array(SPECTRUM_BINS);

// Map 128 FFT bins (Float32Array, 0..1) onto folia's 5-band AudioBands shape.
const compute5Band = (bands: Float32Array) => {
  const peak = (a: number, b: number) => { let m = 0; for (let i = a; i < b && i < bands.length; i++) m = Math.max(m, bands[i]); return m; };
  return {
    bass: peak(0, 6),
    lowMid: peak(6, 20),
    mid: peak(20, 55),
    vocal: peak(55, 90),
    treble: peak(90, 128),
  };
};

// Resolve the active visualizer mode:
//   1) global __FOLIA_MODE__ (pinned to window by preload from config)
//   2) ?mode= query param
//   3) 'classic' default
const resolveMode = (): VisualizerMode => {
  const w = window as unknown as { __FOLIA_MODE__?: string };
  if (w.__FOLIA_MODE__) return w.__FOLIA_MODE__ as VisualizerMode;
  try {
    const p = new URLSearchParams(location.search).get('mode');
    if (p) return p as VisualizerMode;
  } catch { /* offscreen may have no search */ }
  return 'classic';
};

const TRANSPARENT_BG: VisualizerBackgroundConfig = { mode: null, transparent: true };

const PulseRingObsApp: React.FC = () => {
  const { state, theme, getCurrentTimeSec } = usePulseRingSource();
  const mode = useMemo(resolveMode, []);

  const [currentLineIndex, setCurrentLineIndex] = useState(-1);
  const currentLineIndexRef = useRef(-1);
  const linesRef = useRef<Line[]>([]);
  const getTimeRef = useRef(getCurrentTimeSec);
  getTimeRef.current = getCurrentTimeSec;
  linesRef.current = state.lyrics?.lines ?? [];

  const currentTime = useMotionValue(0);
  const audioPower = useMotionValue(0);
  const bass = useMotionValue(0);
  const lowMid = useMotionValue(0);
  const mid = useMotionValue(0);
  const vocal = useMotionValue(0);
  const treble = useMotionValue(0);
  const spectrum = useMotionValue(REUSE_SPECTRUM);
  const audioBands = useMemo(() => ({ bass, lowMid, mid, vocal, treble, spectrum }),
    [bass, lowMid, mid, spectrum, treble, vocal]);

  const paused = !state.clock.playing;

  // Transparent body so the wgpu wallpaper shows through behind/above.
  useEffect(() => {
    document.body.style.backgroundColor = 'transparent';
    document.documentElement.style.backgroundColor = 'transparent';
    document.body.style.overflow = 'hidden';
    document.title = 'pulse-ring · folia';
  }, []);

  // rAF clock + lyric line index (same pattern as ObsWebSourceApp).
  useEffect(() => {
    let frameId = 0;
    const tick = () => {
      const lyricTime = getTimeRef.current(Date.now());
      currentTime.set(lyricTime);
      const lines = linesRef.current;
      const next = lines.length > 0 ? findLatestActiveLineIndex(lines, lyricTime) : -1;
      if (next !== currentLineIndexRef.current) {
        currentLineIndexRef.current = next;
        setCurrentLineIndex(next);
      }
      frameId = window.requestAnimationFrame(tick);
    };
    frameId = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frameId);
  }, [currentTime]);

  // Audio pump: read pulse-ring's FFT each rAF, drive the 5-band MotionValues.
  useEffect(() => {
    const api = window.pulseRing;
    if (!api?.getAudioData) return;
    let frameId = 0;
    const pump = () => {
      const d = api.getAudioData!();
      if (d) {
        audioPower.set(d.energy ?? 0);
        if (d.bands && d.bands.length > 0) {
          const b = compute5Band(typeof d.bands === 'object' ? d.bands : new Float32Array());
          bass.set(b.bass); lowMid.set(b.lowMid); mid.set(b.mid); vocal.set(b.vocal); treble.set(b.treble);
          for (let i = 0; i < SPECTRUM_BINS && i < d.bands.length; i++) REUSE_SPECTRUM[i] = Math.min(255, Math.round(d.bands[i] * 255));
          spectrum.set(REUSE_SPECTRUM);
        }
      }
      frameId = window.requestAnimationFrame(pump);
    };
    frameId = window.requestAnimationFrame(pump);
    return () => window.cancelAnimationFrame(frameId);
  }, [audioPower, bass, lowMid, mid, vocal, treble, spectrum]);

  const { visualizerTheme, visualizerSubtitleTheme } = useMemo(
    () => buildVisualizerTheme({ appStyle: {}, theme, visualizerMode: mode }),
    [theme, mode],
  );

  const coverUrl = state.track?.coverUrl ?? null;

  return (
    <div
      style={{
        width: '100vw',
        height: '100vh',
        backgroundColor: 'transparent',
        color: theme.primaryColor,
      }}
    >
      <VisualizerRenderer
        mode={mode}
        currentTime={currentTime}
        currentLineIndex={currentLineIndex}
        lines={state.lyrics?.lines ?? []}
        theme={visualizerTheme}
        subtitleTheme={visualizerSubtitleTheme}
        audioPower={audioPower}
        audioBands={audioBands}
        songTitle={state.track?.name}
        songArtist={state.track?.artist}
        coverUrl={coverUrl}
        showText={true}
        seed={state.track?.seed || 'pulse-ring-folia'}
        paused={paused}
        visualizerOpacity={1}
        background={TRANSPARENT_BG}
        isPlayerChromeHidden={true}
        hideTranslationSubtitle={false}
        showSubtitleTranslation={true}
      />
    </div>
  );
};

export default PulseRingObsApp;
