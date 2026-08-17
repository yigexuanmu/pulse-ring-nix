// folia-wallpaper/src/main.tsx
//
// pulse-ring · folia OBS browser source entry — folia 同款。
//
// BrowserWindow loadURL('http://127.0.0.1:<port>/obs?obs=1&token=local')
// 所以 window.location.search 永远带 obs=1 → 这条入口等价于 folia 原版
// bootstrap.tsx 的 ?obs=1 路由分支（只渲染 ObsBrowserSourceApp）。
//
// 与 folia bootstrap.tsx 的偏离只在"删了旁支 import":
//   - RemoteControlApp / ObsNowPlayingSourceApp / ObsPlayerCapSourceApp
//     (pulse-ring wallpaper 场景不需要, 项目里也没复制)
//   - initializeLocalCoverRuntime (pulse-ring 走自己的封面线程, folia 那条
//     runtime 在 pulse-ring 里不存在)
// 不影响 ObsBrowserSourceApp 行为 — 它自给自足只用 EventSource + window.location。

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
