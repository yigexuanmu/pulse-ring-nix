// pulse-ring 网页壁纸预加载脚本：把音频/控制 API 暴露给页面
// 页面里可这样使用：
//   window.pulseRing.onAudio(({ bands, energy, bass, mid, treble }) => { ... })  // 每帧音频
//   window.pulseRing.onConfig((cfg) => { ... })      // 壁纸清单参数（可选）
//   window.pulseRing.onLyrics((lyrics) => { ... })   // 已解析歌词 (LyricData 形状)
//   window.pulseRing.onPlayback((pb) => { ... })     // MPRIS 播放进度时钟
//   window.pulseRing.onTheme((theme) => { ... })      // 可视化主题配色
//
// 切换歌词可视化模式：config 在页面里经 onConfig 订阅拿到（例如
// project.json 的 params.visualizerMode），页面再据此切 mode。不能在
// preload 里写 window.__FOLIA_MODE__——contextIsolation 默认开启，preload
// 的 window 是隔离世界，主页面读不到。
const { contextBridge, ipcRenderer } = require('electron');

const subscribe = (channel, map, cb) => {
  if (typeof cb !== 'function') throw new TypeError('pulseRing callback must be a function');
  const listener = (_event, value) => cb(map(value));
  ipcRenderer.on(channel, listener);
  return () => ipcRenderer.removeListener(channel, listener);
};

// 各通道最新值缓存，供 get*State() 在订阅前回读（preload 可能比页面早收到事件）
let latestAudio = Object.freeze({
  bands: new Float32Array(128), energy: 0, bass: 0, mid: 0, treble: 0, timestamp: 0,
});
let latestLyrics = null;
let latestPlayback = null;
let latestTheme = null;
let latestConfig = null;

ipcRenderer.on('pulse-bands', (_event, data) => {
  latestAudio = Object.freeze({
    bands: new Float32Array(data.bands),
    energy: Number(data.energy) || 0,
    bass: Number(data.bass) || 0,
    mid: Number(data.mid) || 0,
    treble: Number(data.treble) || 0,
    timestamp: Number(data.timestamp) || Date.now(),
  });
});

ipcRenderer.on('pulse-lyrics', (_event, data) => { latestLyrics = data; });
ipcRenderer.on('pulse-playback', (_event, data) => { latestPlayback = data; });
ipcRenderer.on('pulse-theme', (_event, data) => { latestTheme = data; });
ipcRenderer.on('pulse-config', (_event, data) => { latestConfig = data; });

const onAudio = (cb) => subscribe('pulse-bands', (data) => ({
  bands: new Float32Array(data.bands),
  energy: Number(data.energy) || 0,
  bass: Number(data.bass) || 0,
  mid: Number(data.mid) || 0,
  treble: Number(data.treble) || 0,
  timestamp: Number(data.timestamp) || Date.now(),
}), cb);

contextBridge.exposeInMainWorld('pulseRing', {
  apiVersion: 1,
  onAudio,
  onBands: onAudio,
  getAudioData: () => latestAudio,
  onConfig: (cb) => subscribe('pulse-config', (cfg) => cfg, cb),
  getConfig: () => latestConfig,
  onLyrics: (cb) => subscribe('pulse-lyrics', (lyrics) => lyrics, cb),
  onPlayback: (cb) => subscribe('pulse-playback', (pb) => pb, cb),
  onTheme: (cb) => subscribe('pulse-theme', (theme) => theme, cb),
  getLyricData: () => latestLyrics,
  getPlaybackState: () => latestPlayback,
  getTheme: () => latestTheme,
});
