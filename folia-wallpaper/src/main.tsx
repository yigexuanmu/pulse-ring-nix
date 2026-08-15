import React from 'react';
import ReactDOM from 'react-dom/client';
import './i18n/config';
import './index.css';
import PulseRingObsApp from './PulseRingObsApp';

// pulse-ring · folia wallpaper entry.
// Electron offscreen renderer loads dist/index.html; this mounts the
// VisualizerRenderer shell driven by window.pulseRing (audio/lyrics/theme).
const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Could not find root element to mount to');
ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <PulseRingObsApp />
  </React.StrictMode>,
);
