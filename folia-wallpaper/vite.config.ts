import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

// Folia lyrics-visualizer compiled to a static web wallpaper for pulse-ring.
// Electron offscreen renderer loads dist/index.html; transparent body so the
// wgpu wallpaper/ring layers show through behind/above.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
  build: {
    target: 'es2020',
    sourcemap: false,
    chunkSizeWarningLimit: 4096,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('/node_modules/three/')) return 'three';
          if (id.includes('/node_modules/pixi.js')) return 'pixi';
          if (id.includes('/node_modules/framer-motion')) return 'framer';
          if (id.includes('/node_modules/react-i18next') || id.includes('/node_modules/i18next')) return 'i18n';
        },
      },
    },
  },
  define: {
    '__APP_VERSION__': JSON.stringify('1.0.0'),
    'process.env.NODE_ENV': JSON.stringify('production'),
  },
});
