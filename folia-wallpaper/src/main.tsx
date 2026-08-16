// folia-wallpaper/src/main.tsx
//
// pulse-ring · folia wallpaper entry.
//
// This Electron offscreen renderer IS the obs scene — it always renders folia's
// original ObsBrowserSourceApp, driven NOT by folia's HTTP SSE server but by our
// in-memory `obs-bridge.ts` which patches window.EventSource and routes the
// preload-exposed `window.pulseRing` (config/lyrics/playback/theme/audio) into
// the same 'config'/'clock'/'audio' SSE event shape that ObsBrowserSourceApp
// expects from `/obs/events`.
//
// The './obs-bridge' import MUST come first (ES module-eval order): its
// `installObsBridge()` runs synchronously here and patches window.EventSource
// before ObsBrowserSourceApp's useEffect constructs `new EventSource(url)`.

import './obs-bridge';
import './i18n/config';
import './index.css';
import React from 'react';
import { createRoot } from 'react-dom/client';
import ObsBrowserSourceApp from './components/obs/ObsBrowserSourceApp';

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Could not find #root element to mount to');

createRoot(rootElement).render(
  <React.StrictMode>
    <ObsBrowserSourceApp />
  </React.StrictMode>,
);
