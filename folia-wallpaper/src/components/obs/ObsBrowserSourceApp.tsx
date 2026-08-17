import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useMotionValue } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import VisualizerRenderer from '../visualizer/VisualizerRenderer';
import { PlayerState } from '../../types';
import type { Theme } from '../../types';
import type { ObsBrowserSourceAudio, ObsBrowserSourceClock, ObsBrowserSourceConfig } from '../../types/obsBrowserSource';
import { findLatestActiveLineIndex } from '../../utils/appPlaybackHelpers';
import { buildObsBrowserSourceConfigSignature, resolveObsBrowserSourceClockTime } from '../../utils/obsBrowserSource';
import { extractColors, extractRepresentativeColors } from '../../utils/colorExtractor';

// src/components/obs/ObsBrowserSourceApp.tsx
// Read-only OBS browser source renderer driven by Folia's main playback clock.
//
// Theme: pulse-ring pushes a fixed theme derived from its ring palette
// (always-purple when no music / single-color preview). Here we override
// `theme.primaryColor / backgroundColor / accentColor` with colors extracted
// from the album cover via the project-bundled `extractColors`, picking a
// mid-lightness representative so the lyric text stays legible on any cover.

const EMPTY_SPECTRUM = new Uint8Array(0);

// --- small color helpers (kept local; no new deps) ---
const hexToRgb = (hex: string): [number, number, number] => {
    const m = hex.replace('#', '');
    return [
        parseInt(m.slice(0, 2), 16) || 0,
        parseInt(m.slice(2, 4), 16) || 0,
        parseInt(m.slice(4, 6), 16) || 0,
    ];
};
const rgbToHex = (r: number, g: number, b: number): string =>
    '#' + [r, g, b]
        .map(v => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0'))
        .join('');
// HSL-ish lightness (range 0-255); used to pick the mid-bright representative.
const rgbLightness = (r: number, g: number, b: number): number =>
    (Math.max(r, g, b) + Math.min(r, g, b)) / 2;
// Saturation 0..1 of a hex color (0=gray, 1=full). Used to pick the cover's
// most-vivid accent whenever the saturation-ranked vibrant palette isn't
// produced (low-saturation covers return nothing vibrant).
const saturation = (hex: string): number => {
    const [r, g, b] = hexToRgb(hex);
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    return max === min ? 0 : (max - min) / (255 - Math.abs(max + min - 255));
};
const pickMostVibrant = (colors: string[]): string | null => {
    if (!colors || colors.length === 0) return null;
    let best: string | null = null;
    let bestSat = -1;
    for (const c of colors) {
        const s = saturation(c);
        if (s > bestSat) { bestSat = s; best = c; }
    }
    return best;
};
// Lighten a color toward white by fraction f (0-1).
const lighten = (hex: string, f: number): string => {
    const [r, g, b] = hexToRgb(hex);
    return rgbToHex(r + (255 - r) * f, g + (255 - g) * f, b + (255 - b) * f);
};
// Darken a color toward black by fraction f (0-1).
const darken = (hex: string, f: number): string => {
    const [r, g, b] = hexToRgb(hex);
    return rgbToHex(r * (1 - f), g * (1 - f), b * (1 - f));
};
// If a color is too dark to read against a very dark background, lift it until
// its lightness clears `minL` (range 0-255). Nighttime goals need a higher
// floor than daytime.
const ensureVisible = (hex: string, minL: number): string => {
    const [r, g, b] = hexToRgb(hex);
    const l = rgbLightness(r, g, b);
    if (l >= minL) return hex;
    const f = (minL - l) / (255 - l || 1);
    return lighten(hex, Math.min(0.85, f));
};
// Derive a theme that matches the cover's actual dominant color:
//   - `representatives` (median-cut, population-weighted) come back sorted by
//     pixel share — [0] is the cover's largest region by area. For dark covers
//     (mostly black/near-black) this maps a near-black [0] to a mid-grey after
//     the visible lift, losing the cover's true accent. Skip representatives
//     lighter than `MIN_PRIMARY_LIGHTNESS` so the chosen primary carries the
//     cover's actual hue identity (e.g. dark blue #5D6C94 beats black #091011).
//   - `vibrants` (saturation-ranked) leggings are accents for emphasis only.
// Returning null preserves the pulse-ring-supplied fallback theme (no flicker).
const MIN_PRIMARY_LIGHTNESS = 30; // 0-255; treat <30 as "near-black background"
const buildCoverTheme = (
    representatives: string[],
    vibrants: string[],
    fallback: Theme,
    isDaylight: boolean,
): Theme | null => {
    if (!representatives || representatives.length === 0) return null;
    // Pick the first representative with visible hue identity; fall back to
    // [0] only when the entire cover is genuinely near-black.
    const firstVisible = representatives.find((hex) => {
        const [r, g, b] = hexToRgb(hex);
        return rgbLightness(r, g, b) >= MIN_PRIMARY_LIGHTNESS;
    }) ?? representatives[0];
    const primaryRaw = firstVisible;
    const accentRaw =
        (vibrants && vibrants.length > 0 ? vibrants[0] : null)
        ?? pickMostVibrant(representatives)
        ?? fallback.accentColor;
    const secondaryRaw =
        representatives.find((hex, i) => i > 0 && hex !== firstVisible)
        ?? (representatives.length > 1 ? representatives[1] : primaryRaw);
    const primaryLift = isDaylight ? 90 : 130;
    const secondaryLift = isDaylight ? 70 : 120;
    const accentLift = isDaylight ? 80 : 100;
    const primaryColor = ensureVisible(primaryRaw, primaryLift);
    return {
        ...fallback,
        name: 'pulse-ring-cover',
        primaryColor,
        accentColor: ensureVisible(accentRaw, accentLift),
        secondaryColor: ensureVisible(secondaryRaw, secondaryLift),
        // Background derived from the true cover primary so the palette reads
        // as one cohesive piece (vs pulse-ring's near-black base). When the
        // chosen primary is itself near-black (genuinely dark covers), keep the
        // background near-black rather than lifting it to grey.
        backgroundColor: rgbLightness(...hexToRgb(primaryRaw)) < MIN_PRIMARY_LIGHTNESS
            ? darken(primaryColor, 0.6)
            : darken(primaryColor, 0.78),
    };
};

const buildEventSourceUrl = () => {
    const params = new URLSearchParams(window.location.search);
    const token = params.get('token') ?? '';
    const devPort = params.get('obsPort');
    const baseUrl = devPort ? `http://127.0.0.1:${devPort}` : window.location.origin;
    const url = new URL('/obs/events', baseUrl);
    url.searchParams.set('token', token);
    return url.toString();
};

const ObsBrowserSourceApp: React.FC = () => {
    const { t } = useTranslation();
    const [config, setConfig] = useState<ObsBrowserSourceConfig | null>(null);
    const [connected, setConnected] = useState(false);
    const [currentLineIndex, setCurrentLineIndex] = useState(-1);
    const [playbackState, setPlaybackState] = useState<PlayerState>(PlayerState.IDLE);
    const [obsScale, setObsScale] = useState(1);
    const [obsDimensions, setObsDimensions] = useState({ width: '100vw', height: '100vh' });
    const currentLineIndexRef = useRef(-1);
    const clockRef = useRef<ObsBrowserSourceClock | null>(null);
    const configRef = useRef<ObsBrowserSourceConfig | null>(null);
    const configSignatureRef = useRef<string | null>(null);
    const currentTime = useMotionValue(0);
    const audioPower = useMotionValue(0);
    const bass = useMotionValue(0);
    const lowMid = useMotionValue(0);
    const mid = useMotionValue(0);
    const vocal = useMotionValue(0);
    const treble = useMotionValue(0);
    const spectrum = useMotionValue(EMPTY_SPECTRUM);
    const audioBands = useMemo(() => ({
        bass,
        lowMid,
        mid,
        vocal,
        treble,
        spectrum,
    }), [bass, lowMid, mid, spectrum, treble, vocal]);

    // Cover-derived override for theme.primaryColor/backgroundColor/accentColor.
    // null preserves the pulse-ring-supplied config.theme (no flicker transition).
    const [derivedTheme, setDerivedTheme] = useState<Theme | null>(null);

    useEffect(() => {
        document.body.style.backgroundColor = 'transparent';
        document.documentElement.style.backgroundColor = 'transparent';
        document.body.style.overflow = 'hidden';
        document.title = 'Folia OBS';
    }, []);

    useEffect(() => {
        configRef.current = config;
    }, [config]);

    // Override lyric/background colors across song changes using the album cover.
    // `extractRepresentativeColors` (median-cut, population-weighted) returns up
    // to 5 hex colors sorted by pixel share — the cover's true main color at [0].
    // `extractColors` returns saturation-ranked vibrant colors used as accent.
    // buildCoverTheme uses the population-weighted primary for `primaryColor`
    // (drives lyric color) so it matches what the user sees on the cover.
    useEffect(() => {
        const coverUrl = config?.coverUrl;
        const isDaylight = config?.isDaylight ?? false;
        const fallback = config?.theme;
        if (!coverUrl || !fallback) {
            setDerivedTheme(null);
            return;
        }
        let cancelled = false;
        void Promise.all([
            extractColors(coverUrl, 5),
            extractRepresentativeColors(coverUrl, 5),
        ]).then(([vibrants, reps]) => {
            if (cancelled) return;
            setDerivedTheme(buildCoverTheme(reps, vibrants, fallback, isDaylight));
        }).catch(() => { if (!cancelled) setDerivedTheme(null); });
        return () => { cancelled = true; };
    }, [config?.coverUrl, config?.isDaylight, config?.theme]);

    useEffect(() => {
        let isHandlingResize = false;
        
        const handleResize = () => {
            if (isHandlingResize) return;
            isHandlingResize = true;
            
            try {
                const isPortrait = window.innerHeight > window.innerWidth;
                const baseWidth = isPortrait ? 1080 : 1920;
                
                // Read the physical dimensions from documentElement to avoid infinite loops when mocking innerWidth
                const realWidth = window.document.documentElement.clientWidth;
                const realHeight = window.document.documentElement.clientHeight;
                
                const scale = Math.max(1, realWidth / baseWidth);
                setObsScale(scale);
                setObsDimensions({
                    width: `${realWidth / scale}px`,
                    height: `${realHeight / scale}px`
                });

                // Globally override devicePixelRatio and window size for the OBS browser source.
                // This forces all child components to naturally render and layout exactly
                // as if they were in a 1920x1080 screen, while natively maintaining 4K text rasterization.
                try {
                    Object.defineProperty(window, 'devicePixelRatio', { get: () => scale, configurable: true });
                    Object.defineProperty(window, 'innerWidth', { get: () => realWidth / scale, configurable: true });
                    Object.defineProperty(window, 'innerHeight', { get: () => realHeight / scale, configurable: true });
                } catch {
                    // Ignore
                }
                
                // Force an event so any visualizer that caches window dimensions updates to the mocked size
                if (scale > 1.0) {
                    window.dispatchEvent(new Event('resize'));
                }
            } finally {
                isHandlingResize = false;
            }
        };
        handleResize();

        window.addEventListener('resize', handleResize);
        return () => window.removeEventListener('resize', handleResize);
    }, []);

    useEffect(() => {
        const eventSource = new EventSource(buildEventSourceUrl());

        eventSource.onopen = () => setConnected(true);
        eventSource.onerror = () => setConnected(false);
        eventSource.addEventListener('config', event => {
            const nextConfig = JSON.parse((event as MessageEvent).data) as ObsBrowserSourceConfig;
            const nextSignature = buildObsBrowserSourceConfigSignature(nextConfig);
            if (nextSignature === configSignatureRef.current) return;
            configSignatureRef.current = nextSignature;
            setConfig(nextConfig);
        });
        eventSource.addEventListener('clock', event => {
            const nextClock = JSON.parse((event as MessageEvent).data) as ObsBrowserSourceClock;
            clockRef.current = nextClock;
            setPlaybackState(prev => (
                prev === nextClock.playerState ? prev : nextClock.playerState
            ));
        });
        eventSource.addEventListener('audio', event => {
            const nextAudio = JSON.parse((event as MessageEvent).data) as ObsBrowserSourceAudio;
            audioPower.set(nextAudio.audioPower);
            bass.set(nextAudio.bands.bass);
            lowMid.set(nextAudio.bands.lowMid);
            mid.set(nextAudio.bands.mid);
            vocal.set(nextAudio.bands.vocal);
            treble.set(nextAudio.bands.treble);
            spectrum.set(new Uint8Array(nextAudio.spectrum));
        });

        return () => {
            eventSource.close();
        };
    }, [audioPower, bass, lowMid, mid, spectrum, treble, vocal]);

    useEffect(() => {
        let frameId = 0;
        const tick = () => {
            const lyricTime = resolveObsBrowserSourceClockTime(clockRef.current);
            
            currentTime.set(lyricTime);

            const lines = configRef.current?.lyrics?.lines ?? [];
            const nextLineIndex = lines.length > 0 ? findLatestActiveLineIndex(lines, lyricTime) : -1;
            if (nextLineIndex !== currentLineIndexRef.current) {
                currentLineIndexRef.current = nextLineIndex;
                setCurrentLineIndex(nextLineIndex);
            }

            frameId = window.requestAnimationFrame(tick);
        };

        frameId = window.requestAnimationFrame(tick);
        return () => window.cancelAnimationFrame(frameId);
    }, [currentTime]);

    if (!config) {
        return (
            <div className="h-screen w-screen bg-transparent grid place-items-center text-white/70 text-sm">
                {connected ? t('obs.waitingForPlayback', 'Waiting for Folia playback') : t('obs.connecting', 'Connecting to Folia')}
            </div>
        );
    }

    const effectiveTheme = derivedTheme ?? config.theme;

    return (
        <div
            className="overflow-hidden"
            style={{
                width: obsDimensions.width,
                height: obsDimensions.height,
                zoom: obsScale,
                backgroundColor: config.background?.transparent ? 'transparent' : effectiveTheme.backgroundColor,
                color: effectiveTheme.primaryColor,
            }}
        >
            <VisualizerRenderer
                mode={config.visualizerMode}
                visualizerTunings={config.visualizerTunings}
                currentTime={currentTime}
                currentLineIndex={currentLineIndex}
                lines={config.lyrics?.lines ?? []}
                theme={effectiveTheme}
                subtitleTheme={config.subtitleTheme}
                isDaylight={config.isDaylight}
                audioPower={audioPower}
                audioBands={audioBands}
                songTitle={config.song?.name}
                songArtist={config.songArtist}
                songAlbum={config.songAlbum}
                coverUrl={config.coverUrl}
                showText={true}
                seed={config.seed}
                staticMode={config.staticMode}
                paused={playbackState !== PlayerState.PLAYING}
                visualizerOpacity={config.visualizerOpacity}
                background={config.background}
                lyricsFontScale={config.lyricsFontScale}
                subtitleOverlayOpacity={config.subtitleOverlayOpacity}
                subtitleOverlayBackground={config.subtitleOverlayBackground ?? true}
                isPlayerChromeHidden={true}
                hideTranslationSubtitle={config.hideTranslationSubtitle}
                showSubtitleTranslation={config.showSubtitleTranslation ?? true}
                subtitleContentMode={config.subtitleContentMode}
                cappellaCustomEmojiImages={config.cappellaCustomEmojiImages}
                cappellaCustomAvatarImages={config.cappellaCustomAvatarImages}
                monetPortraitImage={config.monetPortraitImage}
            />
        </div>
    );
};

export default ObsBrowserSourceApp;
