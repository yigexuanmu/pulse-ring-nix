// pulse-ring 网页壁纸渲染器（Electron 离屏）
//
// 用法：electron main.js <html路径> <宽度> <高度>
// 通过 capturePage 定时抓帧，把页面帧按 stdout 输出：
//   [4 字节 LE 宽][4 字节 LE 高][宽*高*4 字节 RGBA]
// 从 stdin 读取带类型的消息：
//   0x00 + 128 x f32 + energy f32
//   0x01 + u32 JSON 长度 + JSON (config → pulse-config)
//   0x02 + u32 JSON 长度 + JSON (lyrics  → pulse-lyrics)
//   0x03 + u32 JSON 长度 + JSON (playback → pulse-playback)
//   0x04 + u32 JSON 长度 + JSON (theme   → pulse-theme)

const { app, BrowserWindow } = require('electron');
const path = require('path');

// htmlPath / width / height 从环境变量读, 不再用 argv 位置参数.
// 原因: Electron CLI flag (如 --no-sandbox) 不被从 argv 剖除, 会留在
// process.argv 里搅乱位置 — 曾导致 main.js 自己被当成 htmlPath 加载
// (页面渲染出 main.js 源码). 用 env 完全脱离 argv 解析陷阱.
const htmlPath = process.env.PULSE_RING_HTML || '';
const htmlIsUrl = htmlPath.startsWith('http://') || htmlPath.startsWith('https://');
const width = parseInt(process.env.PULSE_RING_WIDTH || '1920', 10);
const height = parseInt(process.env.PULSE_RING_HEIGHT || '1080', 10);
if (!htmlPath) {
  console.error('[pulse-ring wallpaper] PULSE_RING_HTML env not set; refusing to start');
  app.quit();
}

let win = null;
let latestConfig = null;
let latestLyrics = null;
let latestPlayback = null;
let latestTheme = null;

let queue = [];
let writing = false;
let paused = false;
let outputClosed = false;

// The Rust side may stop/restart a wallpaper while Electron is between frames.
// A closed stdout pipe is expected in that case; do not turn it into an
// uncaught EPIPE dialog from Electron's main process.
process.stdout.on('error', () => {
  outputClosed = true;
  queue = [];
  writing = false;
  paused = true;
  process.exit(0);
});

function writeFrame(buf, w, h) {
  // 帧头写入实际尺寸 w/h（而非 argv 的 width/height），原因是 Wayland 离屏后端
  // 下的 capturePage 返回合成尺寸而非 window 尺寸；帧头若与缓冲不匹配，Rust 端
  // 会按错误尺寸分配 → 解析错位。管道忙时丢弃本帧防止内存爆炸。
  if (paused || outputClosed) return;
  const header = Buffer.alloc(8);
  header.writeUInt32LE(w, 0);
  header.writeUInt32LE(h, 4);
  queue.push(header);
  queue.push(buf);
  pump();
}

function pump() {
  if (writing || queue.length === 0 || outputClosed) return;
  writing = true;
  const chunk = queue.shift();
  let ok;
  try {
    ok = process.stdout.write(chunk, (err) => {
      if (err) outputClosed = true;
    });
  } catch (_) {
    outputClosed = true;
    writing = false;
    return;
  }
  if (!ok) {
    paused = true;
    process.stdout.once('drain', () => {
      paused = false;
      writing = false;
      pump();
    });
  } else {
    writing = false;
    pump();
  }
}

// ---- 从 stdin 读取 pulse-ring 推送的音频和配置 ----
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
      const peak = (from, to) => {
        let value = 0;
        for (let i = from; i < to; i++) value = Math.max(value, bands[i]);
        return value;
      };
      if (win && !win.isDestroyed()) {
        win.webContents.send('pulse-bands', {
          bands,
          energy,
          bass: peak(0, 32),
          mid: peak(32, 96),
          treble: peak(96, 128),
          timestamp: Date.now(),
        });
      }
      input.buf = input.buf.slice(AUDIO_BYTES);
      continue;
    }

    // Tags 1-4 share the same envelope: u32 LE JSON length + JSON bytes.
    // 1=config, 2=lyrics, 3=playback, 4=theme. Dropped on parse error.
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
      const channel = { 1: 'pulse-config', 2: 'pulse-lyrics', 3: 'pulse-playback', 4: 'pulse-theme' }[tag];
      if (!channel) continue;
      // 缓存必须在 !win guard 之前: stdin 可能在 app.whenReady 建 win 之前到达,
      // 此时 packet 仍需缓存到 latestXxx, 否则 did-finish-load 重放看到 null,
      // obs-bridge 永远收不到 config/lyrics → 页面停在 Waiting / 经典 fallback.
      if (tag === 1) latestConfig = payload;
      else if (tag === 2) latestLyrics = payload;
      else if (tag === 3) latestPlayback = payload;
      else if (tag === 4) latestTheme = payload;
      if (!win || win.isDestroyed()) continue;   // win 未就绪: 只缓存不发送
      win.webContents.send(channel, payload);
      continue;
    }

    // Unknown byte: discard only that byte so a malformed packet cannot make
    // subsequent valid audio/config packets disappear.
    input.buf = input.buf.slice(1);
  }
});

// Wayland（rootless XWayland 下 X11 后端会因无 root window 触发
// XGetWindowAttributes failed → whenReady 死锁。Wayland 后端 on this
// compositor (niri) 经实测工作）。
app.commandLine.appendSwitch('ozone-platform', 'wayland');
// 不调 disableHardwareAcceleration()：sonnet/monet/diorama/pendolo 需要 WebGL（PixiJS）。
// 之前加它是因为 GPU 进程在 NixOS sandbox 下 exit 139；但主 spawn 已强制
// --no-sandbox（web_wallpaper.rs），sandbox 不再阻碍 GPU 进程初始化，
// 留 WebGL 即可恢复 sonnet PixiJS 渲染。

app.whenReady().then(() => {
  // transparent: 歌词层除歌词特效本身外应当全透明，让底层壁纸透出。
  // RGBA 帧透明区域 alpha=0，Rust overlay pass 用 ALPHA_BLENDING 合成时
  // alpha=0 的像素不贡献，底层壁纸原样保留。只设 transparent 即可，
  // 不要设 backgroundColor（任何不透明底色都会涂死透明区域）。
  win = new BrowserWindow({
    width,
    height,
    show: false,
    frame: false,
    transparent: true,
    webPreferences: {
      offscreen: true,
      backgroundThrottling: false,
      preload: path.join(__dirname, 'preload.js'),
    },
  });
  win.webContents.setFrameRate(30);
  // File target 走 loadFile (本地打包的 folia React bundle);
  // 远程 URL (folia OBS browser source) 走 loadURL — 页面自己从其 SSE 后端
  // 拿歌词/进度/频谱, pulse-ring 只 capturePage 抓帧.
  if (htmlIsUrl) win.loadURL(htmlPath); else win.loadFile(htmlPath);
  win.webContents.on('did-finish-load', () => {
    if (latestConfig) win.webContents.send('pulse-config', latestConfig);
    if (latestLyrics) win.webContents.send('pulse-lyrics', latestLyrics);
    if (latestPlayback) win.webContents.send('pulse-playback', latestPlayback);
    if (latestTheme) win.webContents.send('pulse-theme', latestTheme);
  });

  // 隐藏窗口的离屏 paint 事件只触发前 1-2 帧就停止（Electron 已知行为），
  // 改用 capturePage 定时抓帧：稳定 ~30fps。
  const captureTimer = () => {
    // 定时器链必须永远延续：paused（stdout 忙）时只跳过本帧，绝不断链，
    // 否则管道一忙就永久停帧（表现为"跑一会儿卡住"）。
    if (!win || win.isDestroyed()) return;
    const schedule = () => setTimeout(captureTimer, 33); // ~30fps
    if (paused) { schedule(); return; }
    win.webContents.capturePage()
      .then((image) => {
        const size = image.getSize();
        // 流式发送 capturePage 返回的实际尺寸帧（Wayland 后端下，离屏合成尺寸
        // 不等于 window 尺寸；零尺寸仅出现在首帧未绘完时，跳过即可）。
        // 帧头写入真实 w/h，Rust 端 upload_overlay 同尺寸走快路径、变尺寸重建纹理。
        if (size.width === 0 || size.height === 0) return;
        const bgra = image.toBitmap(); // BGRA（Electron 位图）
        const rgba = Buffer.allocUnsafe(bgra.length);
        for (let i = 0; i < bgra.length; i += 4) {
          rgba[i] = bgra[i + 2];
          rgba[i + 1] = bgra[i + 1];
          rgba[i + 2] = bgra[i];
          rgba[i + 3] = bgra[i + 3];
        }
        writeFrame(rgba, size.width, size.height);
      })
      .catch(() => {})
      .finally(schedule);
  };
  setTimeout(captureTimer, 300); // 等页面加载后开始

  win.webContents.on('did-fail-load', (_e, code, desc) => {
    console.error(`web wallpaper load failed (${code}): ${desc}`);
    process.exit(1);
  });
});

process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));
