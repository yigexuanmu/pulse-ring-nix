// folia-wallpaper/src/obs-bridge.ts
//
// Replaces folia's HTTP-SSE (`new EventSource('/obs/events')`) with an in-memory
// bus living entirely in this Electron renderer's main world. The offscreen
// folia ObsBrowserSourceApp can then render WITHOUT folia's HTTP server running
// AND WITHOUT pulse-ring spawning its own HTTP server.
//
// This module is side-effect only: import './obs-bridge' BEFORE importing
// ObsBrowserSourceApp so the EventSource patch lands before the first render.
//
// flow:
//   Electron preload (contextIsolation) exposes `window.pulseRing`
//     (subscribe callbacks for config/lyrics/playback/theme/audio).
//   installObsBridge() at module-eval time:
//     - replaces `window.EventSource` with MockEventSource (no network)
//     - dispatches an initial empty ObsBrowserSourceConfig into __obsBus so
//       ObsBrowserSourceApp bypasses its `if (!config) return Waiting` early
//       return and renders the transparent backdrop immediately
//     - subscribes window.pulseRing.onConfig/onLyrics/onPlayback/onTheme/onAudio
//   ObsBrowserSourceApp mounts -> useEffect -> new EventSource('/obs/events')
//     -> MockEventSource ctor installs its config/clock/audio listeners into
//     __obsBus and replays the already-dispatched initial config (late-joiner)
//   pulseRing IPCs arrive -> bridge builds fresh
//     ObsBrowserSourceConfig / ObsBrowserSourceClock / ObsBrowserSourceAudio
//     and dispatches into __obsBus -> ObsBrowserSourceApp listeners update
//     React state -> VisualizerRenderer re-renders.

import { PlayerState } from './types';
import type {
  Line,
  LyricData,
  Theme,
  VisualizerMode,
} from './types';
import type { VisualizerTuningBundle } from './components/visualizer/tuningRegistry';
import type { VisualizerBackgroundConfig } from './components/visualizer/backgrounds/definition';
import type {
  ObsBrowserSourceAudio,
  ObsBrowserSourceClock,
  ObsBrowserSourceConfig,
} from './types/obsBrowserSource';
import { migrateLyricLinesRenderHints } from './utils/lyrics/renderHints';
import type {
  PulseRingApi,
  PulseRingAudio,
  PulseRingConfig,
  PulseRingLyricData,
  PulseRingPlayback,
  PulseRingTheme,
} from './pulseRing';

// ---------- internal in-memory SSE bus ----------

type ObsSseType = 'config' | 'clock' | 'audio';
type SseListener = (ev: MessageEvent) => void;

interface ObsBus {
  // cached last dispatch per type, so listeners that attach AFTER the initial
  // dispatch still see it. Real SSE doesn't replay — but our initial config
  // is fired synchronously at install time, before ObsBrowserSourceApp's
  // useEffect can subscribe, so the replay path is what makes the renderer
  // bypass the "Waiting for Folia playback" placeholder.
  lastByType: Record<ObsSseType, MessageEvent | null>;
  listenersByType: Record<ObsSseType, Set<SseListener>>;
  dispatch: (type: ObsSseType, payload: unknown) => void;
}

const obsBus: ObsBus = {
  lastByType: { config: null, clock: null, audio: null },
  listenersByType: {
    config: new Set<SseListener>(),
    clock: new Set<SseListener>(),
    audio: new Set<SseListener>(),
  },
  dispatch(type, payload) {
    const ev = new MessageEvent(type, { data: JSON.stringify(payload) });
    obsBus.lastByType[type] = ev;
    // Snapshot before iterating — a listener may remove itself or close peers.
    for (const fn of Array.from(obsBus.listenersByType[type])) {
      try {
        fn(ev);
      } catch (e) {
        console.error('[obs-bridge] listener error', e);
      }
    }
  },
};

// Exposed for diagnostics: any page-level executeJavaScript can read this to inspect
// what the bridge actually saw from window.pulseRing and what it dispatched to ObsBrowserSourceApp.
(window as unknown as {
  __obsBus?: unknown;
  __obsBridgeDebug?: () => unknown;
}).__obsBus = obsBus;
(window as unknown as { __obsBridgeDebug?: () => unknown }).__obsBridgeDebug = () => ({
  initialConfigDispatched,
  cachedConfig,
  cachedLyricsLines: cachedLyrics?.lines?.length ?? 0,
  cachedPlayback: cachedPlayback && {
    title: cachedPlayback.title,
    positionSec: cachedPlayback.positionSec,
    durationSec: cachedPlayback.durationSec,
    playing: cachedPlayback.playing,
    coverUrl: cachedPlayback.coverUrl,
  },
  cachedTheme: cachedTheme && { name: cachedTheme.name, primaryColor: cachedTheme.primaryColor },
  lastDispatched: {
    config: obsBus.lastByType.config && JSON.parse(obsBus.lastByType.config.data as string).visualizerMode,
    clockSentAtMs: obsBus.lastByType.clock && JSON.parse(obsBus.lastByType.clock.data as string).sentAtMs,
    audioSentAtMs: obsBus.lastByType.audio && JSON.parse(obsBus.lastByType.audio.data as string).sentAtMs,
  },
  obsBusListenerCounts: {
    config: obsBus.listenersByType.config.size,
    clock: obsBus.listenersByType.clock.size,
    audio: obsBus.listenersByType.audio.size,
  },
});

// ---------- MockEventSource: drop-in replacement for window.EventSource ----------

class MockEventSource {
  readonly url: string;
  readonly withCredentials = false;
  onopen: ((this: MockEventSource, ev: Event) => any) | null = null;
  onerror: ((this: MockEventSource, ev: Event) => any) | null = null;
  onmessage: ((this: MockEventSource, ev: MessageEvent) => any) | null = null;
  readyState = 0;
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;

  // listeners this instance registered — used by close() to revoke only its own
  // subscriptions (parallel EventSource instances won't trample each other).
  private readonly myListeners: Record<ObsSseType, Set<SseListener>> = {
    config: new Set<SseListener>(),
    clock: new Set<SseListener>(),
    audio: new Set<SseListener>(),
  };

  constructor(url: string | URL) {
    this.url = String(url);
    // Async 'open' so ObsBrowserSourceApp's onopen handler flips connected=true
    // after a microtask — same lifecycle as a real EventSource connecting.
    setTimeout(() => {
      if (this.readyState === 2) return;
      this.readyState = 1;
      this.onopen?.(new Event('open'));
    }, 0);
  }

  addEventListener(type: string, listener: SseListener) {
    if (type !== 'config' && type !== 'clock' && type !== 'audio') return;
    this.myListeners[type].add(listener);
    obsBus.listenersByType[type].add(listener);
    // Late-joiner: replay the cached last payload of this type to the newly
    // registered listener immediately. Without this, the initial config
    // dispatched synchronously at install time would never reach ObsBrowserSourceApp.
    const cached = obsBus.lastByType[type];
    if (cached) {
      try {
        listener(cached);
      } catch (e) {
        console.error('[obs-bridge] replay error', e);
      }
    }
  }

  removeEventListener(type: string, listener: SseListener) {
    if (type === 'config' || type === 'clock' || type === 'audio') {
      this.myListeners[type].delete(listener);
      obsBus.listenersByType[type].delete(listener);
    }
  }

  dispatchEvent(_ev: Event) {
    return true;
  }

  close() {
    this.readyState = 2;
    for (const t of ['config', 'clock', 'audio'] as ObsSseType[]) {
      for (const l of this.myListeners[t]) obsBus.listenersByType[t].delete(l);
      this.myListeners[t].clear();
    }
  }
}

// Patch the global EventSource BEFORE ObsBrowserSourceApp's first useEffect runs.
(window as unknown as { EventSource?: unknown }).EventSource = MockEventSource;

// ---------- audio band + theme ports (lifted from the removed PulseRingObsApp) ----------

const SPECTRUM_BIN_LIMIT = 128;

function compute5Band(bands: Float32Array) {
  const peak = (a: number, b: number) => {
    let m = 0;
    for (let i = a; i < b && i < bands.length; i++) m = Math.max(m, bands[i]);
    return m;
  };
  return {
    bass: peak(0, 6),
    lowMid: peak(6, 20),
    mid: peak(20, 55),
    vocal: peak(55, 90),
    treble: peak(90, 128),
  };
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

const TRANSPARENT_BG: VisualizerBackgroundConfig = { mode: null, transparent: true };

function toLyricData(raw: PulseRingLyricData | null): LyricData | null {
  if (!raw || !raw.lines || raw.lines.length === 0) return null;
  const lines: Line[] = raw.lines.map((l) => ({
    startTime: l.startTime,
    endTime: l.endTime,
    fullText: l.fullText,
    words: (l.words || []).map((w) => ({
      startTime: w.startTime,
      endTime: w.endTime,
      text: w.text,
    })),
    translation: l.translation,
    isChorus: l.isChorus,
    backgroundVocals: [],
  }));
  const { value } = migrateLyricLinesRenderHints(lines);
  return { lines: value };
}

function toTheme(raw: PulseRingTheme | null, fallback: Theme): Theme {
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
}

// ---------- cached PulseRing state + builders ----------

let cachedConfig: PulseRingConfig | null = null;
let cachedLyrics: PulseRingLyricData | null = null;
let cachedPlayback: PulseRingPlayback | null = null;
let cachedTheme: PulseRingTheme | null = null;
let initialConfigDispatched = false;

function buildObsConfig(): ObsBrowserSourceConfig {
  const theme = toTheme(cachedTheme, DEFAULT_FALLBACK_THEME);
  const lyrics = toLyricData(cachedLyrics);
  const mode: VisualizerMode =
    ((cachedConfig?.visualizerMode as string | undefined) ?? 'classic') as VisualizerMode;
  const tunings = cachedConfig?.foliaTuning as VisualizerTuningBundle | undefined;
  const hasTrack = !!cachedPlayback;

  return {
    activePlaybackContext: 'main',
    stageSource: null,
    hasTrack,
    song: hasTrack
      ? {
          id: cachedPlayback!.seed || cachedPlayback!.title,
          name: cachedPlayback!.title,
        }
      : null,
    songArtist: cachedPlayback?.artist ?? null,
    songAlbum: cachedPlayback?.album ?? null,
    coverUrl: cachedPlayback?.coverUrl ?? null,
    lyrics,
    theme,
    isDaylight: false,
    visualizerMode: mode,
    visualizerTunings: tunings,
    background: TRANSPARENT_BG,
    lyricsFontScale: 1,
    visualizerOpacity: 1,
    subtitleOverlayOpacity: 1,
    subtitleOverlayBackground: true,
    staticMode: false,
    hideTranslationSubtitle: false,
    showSubtitleTranslation: true,
    seed: cachedPlayback?.seed || 'pulse-ring-folia',
    updatedAt: Date.now(),
  };
}

function dispatchConfig() {
  obsBus.dispatch('config', buildObsConfig());
}

function dispatchClock(pb: PulseRingPlayback) {
  const clock: ObsBrowserSourceClock = {
    currentTime: pb.positionSec,
    duration: pb.durationSec,
    playerState: pb.playing ? PlayerState.PLAYING : PlayerState.PAUSED,
    sentAtMs: Date.now(),
    playbackRate: 1,
  };
  obsBus.dispatch('clock', clock);
}

function dispatchAudio(d: PulseRingAudio) {
  const arr: Float32Array =
    d.bands && typeof d.bands.length === 'number' ? d.bands : new Float32Array();
  const spectrum: number[] = [];
  for (let i = 0; i < arr.length && i < SPECTRUM_BIN_LIMIT; i++) {
    spectrum.push(Math.min(255, Math.round(arr[i] * 255)));
  }
  const audio: ObsBrowserSourceAudio = {
    audioPower: d.energy ?? 0,
    bands: compute5Band(arr),
    spectrum,
    sentAtMs: Date.now(),
  };
  obsBus.dispatch('audio', audio);
}

// ---------- install: patch + subscribe ----------

/**
 * Installs the in-memory SSE bridge. Idempotent: re-calling from outside is
 * harmless — it only re-reads cached state and re-dispatches the initial config.
 */
export function installObsBridge(): void {
  // pulseRing.d.ts is a module so its `interface Window { pulseRing? ... }`
  // does not extend the global Window; cast to read the bridge directly.
  const api: PulseRingApi | undefined = (
    window as unknown as { pulseRing?: PulseRingApi }
  ).pulseRing;
  if (!api) {
    // No pulseRing preload (e.g. browsing the page outside Electron). Still push
    // an initial config so the renderer shows the default theme instead of a
    // permanent "Connecting to Folia" placeholder.
    if (!initialConfigDispatched) {
      initialConfigDispatched = true;
      dispatchConfig();
    }
    return;
  }

  // Read any state the preload has already cached from earlier IPCs.
  cachedConfig = api.getConfig?.() ?? null;
  cachedLyrics = api.getLyricData?.() ?? null;
  cachedPlayback = api.getPlaybackState?.() ?? null;
  cachedTheme = api.getTheme?.() ?? null;

  // First paint: dispatch a valid (empty-track) config so ObsBrowserSourceApp
  // bypasses its `if (!config) return Waiting` early return immediately.
  if (!initialConfigDispatched) {
    initialConfigDispatched = true;
    dispatchConfig();
  }

  // Whenever underlying state changes, rebuild & re-publish.
  api.onConfig?.((cfg) => {
    cachedConfig = cfg;
    dispatchConfig();
  });
  api.onLyrics?.((ly) => {
    cachedLyrics = ly;
    dispatchConfig();
  });
  api.onPlayback?.((pb) => {
    cachedPlayback = pb;
    if (pb) dispatchClock(pb);
    // Track info (songArtist/coverUrl/seed) lives on playback; refresh config
    // so ObsBrowserSourceApp recomposes `config.song`/`config.coverUrl` etc.
    dispatchConfig();
  });
  api.onTheme?.((th) => {
    cachedTheme = th;
    dispatchConfig();
  });
  api.onAudio?.((d) => {
    dispatchAudio(d);
  });
}

// Side-effect: run immediately on import. main.tsx imports './obs-bridge' BEFORE
// importing ObsBrowserSourceApp, so the EventSource patch is in place by the
// time ObsBrowserSourceApp's first useEffect runs.
installObsBridge();
