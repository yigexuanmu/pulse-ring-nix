// pulse-ring 网页壁纸渲染器（Electron + wl-paper layer-shell）— folia OBS 浏览器源同款
//
// F-wl-paper 架构：electron 不再 offscreen 抓帧走 stdout，而是被 wl-paper 包成
// Wayland layer-shell surface 直接可见。compositor 自己合成 folia 层与 pulse-ring
// background 层，pulse-ring 不再 wgpu 上传 folia 帧。
//
//   pulse-ring (Rust) — stdin → wl-paper → main.js —→ 本进程内嵌 mini HTTP+SSE server
//                                                           ├─ GET /obs?obs=1&token=local → folia dist/index.html
//                                                           └─ GET /obs/events?token=local → SSE 长连接
//                                                                  event: config/clock/audio
//                                                          ↑
//   BrowserWindow loadURL('http://127.0.0.1:port/obs?obs=1&token=local')  (可见窗口, wlpaper 转 layer surface)
//     ├─ folia 原版 ObsBrowserSourceApp 用原生 new EventSource('/obs/events?token=local') 连 SSE
//     └─ Chromium 渲染 sonnet/... 直接画到自己的 GPU surface (compositor 合层)
//
// stdin 协议（不改，仍是 Rust 端原有 4 个 tag）：
//   0x00 + 128 x f32 + energy f32               (audio)
//   0x01 + u32 JSON 长度 + JSON                  (pulse-config)
//   0x02 + u32 JSON 长度 + JSON                  (pulse-lyrics)
//   0x03 + u32 JSON 长度 + JSON                  (pulse-playback)
//   0x04 + u32 JSON 长度 + JSON                  (pulse-theme)
//
// stdout 协议（不改）：
//   [4 字节 LE 宽][4 字节 LE 高][宽*高*4 字节 BGRA]

const { app, BrowserWindow } = require('electron');
const path = require('path');
const http = require('http');
const fs = require('fs');
const url = require('url');

// htmlPath / width / height 从环境变量读, 不再用 argv 位置参数.
const htmlPath = process.env.PULSE_RING_HTML || '';
const htmlIsUrl = htmlPath.startsWith('http://') || htmlPath.startsWith('https://');

// 渲染分辨率走 PULSE_RING_WIDTH/HEIGHT（Rust spawn 时从 cfg.web_wallpaper_size 传入）。
// 不自适应整屏 —— 每帧 raw BGRA 走 stdout pipe ~39 MiB/s，窗口尺寸决定 fps 上限：
//   960×540  → 2 MiB/帧 → ~57fps
//   1920×1080 → 7.9 MiB/帧 → ~5fps
// Rust overlay pass 在 GPU 上 bilinear 上采样 web 帧铺满整屏，web 端只需标清即可。
const fallbackWidth  = parseInt(process.env.PULSE_RING_WIDTH  || '960', 10);
const fallbackHeight = parseInt(process.env.PULSE_RING_HEIGHT || '540', 10);

// ─────────────────────────────────────────────────────────────────────────────
// folia OBS 浏览器源 SSE server（folia 原版 main.cjs:2484 同款格式）
//
// SSE 事件格式 (具名事件):
//   event: <name>\n
//   data: <JSON>\n\n
// ObsBrowserSourceApp 用 addEventListener('config'/'clock'/'audio') 消费。
// ─────────────────────────────────────────────────────────────────────────────

const OBS_TOKEN = 'local';
const SSE_CONTENT_TYPE = 'text/event-stream; charset=utf-8';

// 推 3 类事件，最新值缓存好让晚加入的 client（页面 reload / 离屏窗口重启）
// 上线时立即收到 bootstrap 事件——folia 原版 sendObsBrowserSourceBootstrapEvents 行为。
let latestEvents = {
  config: null,
  clock: null,
  audio: null,
};
let sseClients = new Set();

function sendSseEvent(res, eventName, payload) {
  res.write(`event: ${eventName}\n`);
  res.write(`data: ${JSON.stringify(payload)}\n\n`);
}

function broadcastSseEvent(eventName, payload) {
  // 缓存最新事件，晚加入的 client 能补收 bootstrap。
  latestEvents[eventName] = payload;
  for (const res of Array.from(sseClients)) {
    try { sendSseEvent(res, eventName, payload); } catch (_) { sseClients.delete(res); }
  }
}

function sendSseBootstrap(res) {
  // folia 原版: 连上就立即推已缓存的 config/clock/audio，避免白屏等待。
  if (latestEvents.config) sendSseEvent(res, 'config', latestEvents.config);
  if (latestEvents.clock)   sendSseEvent(res, 'clock', latestEvents.clock);
  if (latestEvents.audio)   sendSseEvent(res, 'audio', latestEvents.audio);
}

// ─────────────────────────────────────────────────────────────────────────────
// pulseRing 数据 → folia ObsBrowserSourceConfig/Clock/Audio 转换
// （从已删的 folia-wallpaper/src/obs-bridge.ts 移植为纯 Node.js）
// ─────────────────────────────────────────────────────────────────────────────

// cached pulseRing raw 状态（preload 已删，数据全留在 main.js 主进程处理）
let cachedConfig = null;    // pulse-config: {visualizerMode, foliaTuning, ...}
let cachedLyrics = null;    // pulse-lyrics: {lines:[{startTime,endTime,fullText,words,translation,isChorus}], offset}
let cachedPlayback = null;  // pulse-playback: {positionSec,durationSec,playing,title,artist,album,coverUrl,seed}
let cachedTheme = null;     // pulse-theme: {name,backgroundColor,primaryColor,accentColor,secondaryColor,fontStyle,...}
let currentSongKey = null;  // 用于判断 song 是否变化（决定是否额外重推 config）

// folia 原版 DEFAULT_SONNET 不需要——sonnet 自身的 Pixi app 初始化有完整 fallback。
// 转换层只负责把 pulseRing 字段映射进 folia ObsBrowserSourceConfig 字段结构。

const DEFAULT_FALLBACK_THEME = {
  name: 'pulse-ring',
  backgroundColor: '#060512',
  primaryColor: '#EADDFF',
  accentColor: '#FFD740',
  secondaryColor: '#B8B4C8',
  fontStyle: 'sans',
  animationIntensity: 'normal',
};

const TRANSPARENT_BG = { mode: null, transparent: true };

// 5 频段峰值（folia ObsBrowserSourceAudio.bands 的 5 字段: bass/lowMid/mid/vocal/treble）
// 间隔与 obs-bridge.ts 对齐 (0-6/6-20/20-55/55-90/90-128)。
function compute5Band(bands) {
  // folia 协议: bands 是 0..255 (AnalyserNode byte range); pulse-ring 给 0..1, *255 对齐.
  const peak = (a, b) => {
    let m = 0;
    for (let i = a; i < b && i < bands.length; i++) m = Math.max(m, bands[i]);
    return Math.min(255, Math.round(m * 255));
  };
  return {
    bass: peak(0, 6),
    lowMid: peak(6, 20),
    mid: peak(20, 55),
    vocal: peak(55, 90),
    treble: peak(90, 128),
  };
}

function toTheme(raw) {
  if (!raw) return DEFAULT_FALLBACK_THEME;
  return {
    name: raw.name || 'pulse-ring',
    backgroundColor: raw.backgroundColor ?? DEFAULT_FALLBACK_THEME.backgroundColor,
    primaryColor: raw.primaryColor ?? DEFAULT_FALLBACK_THEME.primaryColor,
    accentColor: raw.accentColor ?? DEFAULT_FALLBACK_THEME.accentColor,
    secondaryColor: raw.secondaryColor ?? DEFAULT_FALLBACK_THEME.secondaryColor,
    fontStyle: raw.fontStyle ?? 'sans',
    fontFamily: raw.fontFamily,
    fontFamilyStack: raw.fontFamilyStack,
    fontWeight: raw.fontWeight,
    animationIntensity: raw.animationIntensity ?? 'normal',
    wordColors: raw.wordColors,
    lyricsIcons: raw.lyricsIcons,
  };
}

// pulseRing 的 PulseRingLyricData → folia 的 LyricData 形状。
// 不 migrate renderHints——folia ObsBrowserSourceApp 不 migrate，
// visualizer 模式 (cadenza/fume 等) 自己用 getLineRenderHints(line) 现场算兜底。
function toLyricData(raw) {
  if (!raw || !raw.lines || raw.lines.length === 0) return null;
  const lines = raw.lines.map((l) => ({
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
  return { lines };
}

function buildObsConfig() {
  const theme = toTheme(cachedTheme);
  const lyrics = toLyricData(cachedLyrics);
  const mode = (cachedConfig && cachedConfig.visualizerMode) || 'classic';
  const tunings = (cachedConfig && cachedConfig.foliaTuning) || undefined;
  const hasTrack = !!cachedPlayback;

  return {
    activePlaybackContext: 'main',
    stageSource: null,
    hasTrack,
    // folia SongResult.id 类型 = string | number, seed 或 title 都合规
    song: hasTrack ? { id: cachedPlayback.seed || cachedPlayback.title, name: cachedPlayback.title } : null,
    songArtist: cachedPlayback ? cachedPlayback.artist : null,
    songAlbum: cachedPlayback ? cachedPlayback.album : null,
    coverUrl: cachedPlayback ? cachedPlayback.coverUrl : null,
    lyrics,
    theme,
    isDaylight: false,
    subtitleTheme: undefined,
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
    seed: (cachedPlayback && cachedPlayback.seed) || 'pulse-ring-folia',
    updatedAt: Date.now(),
  };
}

function buildObsClock() {
  if (!cachedPlayback) return null;
  return {
    currentTime: cachedPlayback.positionSec || 0,
    duration: cachedPlayback.durationSec || 0,
    playerState: cachedPlayback.playing ? 'PLAYING' : 'PAUSED',
    sentAtMs: Date.now(),
    playbackRate: 1,
  };
}

function buildObsAudio(bands, energy) {
  const arr = (bands && typeof bands.length === 'number') ? bands : [];
  const spectrum = [];
  for (let i = 0; i < arr.length && i < 128; i++) {
    spectrum.push(Math.min(255, Math.round(arr[i] * 255)));
  }
  // folia 协议: audioPower/bands/spectrum 全部 0..255 (folia usePlaybackVisualizerBridge 用
  // AnalyserNode.getByteFrequencyData 0..255 再 process() 保持 0..255; diorama DioramaScene.tsx:879
  // 注释 "Audio levels arrive 0..255" 后 /255 归一). pulse-ring audio.rs 给 0..1 FFT magnitude,
  // 这里 *255 对齐; 否则 diorama /255 永远 ~0.0003 → 音频响应失效.
  return {
    audioPower: Math.min(255, Math.round((energy || 0) * 255)),
    bands: compute5Band(arr),
    spectrum,
    sentAtMs: Date.now(),
  };
}

// song 变化判断：title+artist+album 的简短签名。
function songKeyOf(pb) {
  if (!pb) return null;
  return `${pb.title || ''}|${pb.artist || ''}|${pb.album || ''}`;
}

// 每次原始 pulseRing 事件到达 → 重算对应 folia 事件并广播。
function handlePulseConfig(cfg) {
  cachedConfig = cfg;
  broadcastSseEvent('config', buildObsConfig());
}
function handlePulseLyrics(ly) {
  cachedLyrics = ly;
  broadcastSseEvent('config', buildObsConfig());
}
function handlePulsePlayback(pb) {
  const oldKey = currentSongKey;
  cachedPlayback = pb;
  const clock = buildObsClock();
  if (clock) broadcastSseEvent('clock', clock);
  // song 变化时 config.song/coverUrl 也变了 → 重推 config。
  const newKey = songKeyOf(pb);
  if (newKey !== oldKey) {
    currentSongKey = newKey;
    broadcastSseEvent('config', buildObsConfig());
  }
}
function handlePulseTheme(th) {
  cachedTheme = th;
  broadcastSseEvent('config', buildObsConfig());
}
function handlePulseAudio(bands, energy) {
  broadcastSseEvent('audio', buildObsAudio(bands, energy));
}

// ─────────────────────────────────────────────────────────────────────────────
// 从 stdin 读取 pulse-ring 推送的音频和配置
// ─────────────────────────────────────────────────────────────────────────────

let win = null;
if (!htmlPath) {
  console.error('[pulse-ring wallpaper] PULSE_RING_HTML env not set; refusing to start');
  app.quit();
}

// F-wl-paper: 不再有 stdout 帧管道。electron 渲染到自己的 GPU surface，
// 由 wl-paper 转成 layer-shell 后 compositor 直接合成，pulse-ring stdout 不再
// 承载 RGBA 帧。仍保留对 stdout 的错误处理，避免意外写入导致 EPIPE 杀进程。
process.stdout.on('error', () => { process.exit(0); });

const input = { buf: Buffer.alloc(0) };
const AUDIO_BYTES = 1 + (128 + 1) * 4;
process.stdin.on('data', (chunk) => {
  input.buf = Buffer.concat([input.buf, chunk]);
  while (input.buf.length > 0) {
    const tag = input.buf[0];
    if (tag === 0) {
      if (input.buf.length < AUDIO_BYTES) break;
      const bands = new Array(128);
      for (let i = 0; i < 128; i++) bands[i] = input.buf.readFloatLE(1 + i * 4);
      const energy = input.buf.readFloatLE(1 + 128 * 4);
      // folia ObsBrowserSourceAudio.bands.bass 用的是 0-6 区间峰值的精细分段，
      // 这里把 stdin 的 128 频段原样传给 handlePulseAudio 做精细 5band 划分
      // （与已删的 obs-bridge compute5Band 完全一致）。
      handlePulseAudio(bands, energy);
      input.buf = input.buf.slice(AUDIO_BYTES);
      continue;
    }

    if (tag >= 1 && tag <= 4) {
      if (input.buf.length < 5) break;
      const len = input.buf.readUInt32LE(1);
      if (len === 0 || len > 1024 * 1024) {
        input.buf = input.buf.slice(1);
        continue;
      }
      if (input.buf.length < 5 + len) break;
      let payload = null;
      try { payload = JSON.parse(input.buf.slice(5, 5 + len).toString('utf8')); } catch (_) {}
      input.buf = input.buf.slice(5 + len);
      // 缓存必须在 win guard 之前: stdin 可能在 app.whenReady 之前到达,
      // 仍需缓存供 did-finish-load 后 bootstrap 重放。同时立即转 SSE 广播,
      // 让已连上的 client 实时收 —— 即使 win 还没就绪, SSE server 已起来。
      if (tag === 1) { if (payload) handlePulseConfig(payload); }
      else if (tag === 2) { if (payload) handlePulseLyrics(payload); }
      else if (tag === 3) { if (payload) handlePulsePlayback(payload); }
      else if (tag === 4) { if (payload) handlePulseTheme(payload); }
      continue;
    }

    input.buf = input.buf.slice(1);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// Electron 主进程: wayland layer-shell (经 wl-paper) + 内嵌 SSE server + BrowserWindow loadURL
// ─────────────────────────────────────────────────────────────────────────────

app.commandLine.appendSwitch('ozone-platform', 'wayland');

function startObsServer(distRoot) {
  return new Promise((resolve, reject) => {
    // loopback-only (127.0.0.1) + same-process → 不做 token 校验, 避免相对路径
    // module script 加载丢 query (=丢 token) 导致 401。
    const srv = http.createServer((req, res) => {
      const reqUrl = url.parse(req.url, true);
      const pathname = reqUrl.pathname;
      console.error(`[obs-srv] ${req.method} ${req.url}`);
      // folia 原版 main.cjs:2484 /obs/events 路由 → SSE 长连接
      if (pathname === '/obs/events') {
        res.writeHead(200, {
          'Content-Type': SSE_CONTENT_TYPE,
          'Cache-Control': 'no-store',
          'Connection': 'keep-alive',
          'X-Accel-Buffering': 'no',
          'Access-Control-Allow-Origin': '*',
        });
        res.write(': connected\n\n');
        sseClients.add(res);
        sendSseBootstrap(res);
        req.on('close', () => { sseClients.delete(res); });
        return;
      }
      // folia 原版: /obs → 静态 dist/index.html；其余路径 → dist 下静态资源。
      // loopback 单进程 server, dist 全公开静态资源 — 不做 path-traversal 校验
      // (原校验在 distRoot 边界条件下误判 /obs 为越界, 渲染出 "Forbidden" 错误页)。
      const relPath = (pathname === '/' || pathname === '/obs')
        ? '/index.html'
        : decodeURIComponent(pathname);
      const filePath = path.resolve(distRoot, '.' + relPath);
      fs.readFile(filePath, (err, data) => {
        if (err) { console.error(`[obs-srv] 404: ${filePath}`); res.writeHead(404); res.end('Not found'); return; }
        const ext = path.extname(filePath).toLowerCase();
        const ct = {
          '.html': 'text/html; charset=utf-8',
          '.js': 'text/javascript; charset=utf-8',
          '.css': 'text/css; charset=utf-8',
          '.json': 'application/json; charset=utf-8',
          '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml',
          '.woff': 'font/woff', '.woff2': 'font/woff2', '.ttf': 'font/ttf',
        }[ext] || 'application/octet-stream';
        console.error(`[obs-srv] 200 ${ext} ${pathname}`);
        res.writeHead(200, {
          'Content-Type': ct,
          'Cache-Control': ext === '.html' ? 'no-store' : 'public, max-age=31536000, immutable',
        });
        res.end(data);
      });
    });
    srv.listen(0, '127.0.0.1', () => {
      const port = srv.address().port;
      resolve(port);
    });
    srv.on('error', reject);
  });
}

app.whenReady().then(async () => {
  const width = fallbackWidth;
  const height = fallbackHeight;

  win = new BrowserWindow({
    width,
    height,
    // F-wl-paper: show:true 让 wl-paper 能抓到可见的 wl_surface。wl-paper 把这个
    // 普通 xdg_toplevel 转成 zwlr_layer_surface 后，compositor 直接合成到屏幕。
    // transparent:true 保留 (folia ObsBrowserSourceApp body 透明，露出底层 pulse-ring background)。
    show: true,
    frame: false,
    transparent: true,
    hasShadow: false,
    backgroundColor: '#00000000',
    webPreferences: {
      // 无 offscreen —— 帧不再过 stdout。compositor 直接收 electron 的 wl_surface commit。
      backgroundThrottling: false,
      // 无 preload —— folia ObsBrowserSourceApp 自给自足, 只用 EventSource + window.location.
    },
  });

  let pageUrl;
  if (htmlIsUrl) {
    // 已是远程 folia OBS 源 URL — 直接 loadURL, pulse-ring 不接管 SSE 服务。
    pageUrl = htmlPath;
  } else {
    // 本地 folia-wallpaper/dist/index.html: 起本地 mini SSE server, loadURL 它,
    // ObsBrowserSourceApp 用原生 new EventSource('/obs/events?token=local') 连本进程 server。
    // distRoot = htmlPath 所在目录 (dist/), server 返回 index.html + 静态资源。
    const distRoot = path.resolve(path.dirname(htmlPath));
    console.error(`[obs-srv] distRoot=${distRoot} htmlPath=${htmlPath}`);
    const port = await startObsServer(distRoot);
    pageUrl = `http://127.0.0.1:${port}/obs?obs=1&token=${OBS_TOKEN}`;
    console.error(`[pulse-ring wallpaper] OBS browser source listening on ${pageUrl}`);
  }

  win.loadURL(pageUrl);

  // 无 paint 事件、无 stdout 帧 — electron 直接渲染到自己的 wl_surface 由
  // wl-paper 转成 layer-shell 后供 compositor 合层。

  win.webContents.on('did-finish-load', () => {
    // SSE 路径下数据通过 HTTP SSE 广播; did-finish-load 时若已有缓存事件,
    // 新连的 client 会通过 sendSseBootstrap 自动补收, 这里无需再主动 send.
  });

  win.webContents.on('did-fail-load', (_e, code, desc) => {
    console.error(`web wallpaper load failed (${code}): ${desc}`);
    process.exit(1);
  });
});

process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
