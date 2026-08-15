# Folia 歌词可视化集成（Neo 分支）

在 pulse-ring 的 Wayland 壁纸层上渲染 [folia-major](https://github.com/chthollyphile/folia-major) 的全部 11 种歌词可视化模式，作为「壁纸」和「中心律动环」之间的中间图层。

```
┌──────────────────────────────────────┐
│  中心律动环 (wgpu 环 + 粒子/widget)    │  ← 最上层
├──────────────────────────────────────┤
│  Folia 歌词层 (Electron 离屏 + React) │  ← 本集成（透明背景）
├──────────────────────────────────────┤
│  壁纸层 (图片/视频/网页/轮播)           │  ← 底层
└──────────────────────────────────────┘
```

- **地铁站名般的 11 个模式**：classic / cadenza / partita / fume / claddagh / cappella / tilt / monet / diorama / pendolo / sonnet
- 数据全由 pulse-ring 自带管线驱动：MPRIS 歌曲信息 + 自带的歌词解析（LRC/增强 LRC）+ playerctl 封面 + PipeWire 128 频段 FFT。folia 端**不做任何联网 / 搜索 / 在线歌词拉取**。

---

## 一、启用（NixOS / Home-Manager）

### 1. 拉取本仓库 Neo 分支

```bash
git clone -b neo https://github.com/yigexuanmu/pulse-ring-nix.git
cd pulse-ring-nix
```

### 2. 直接用 flake 接入

把本 flake 作为 overlay / package 接入你的 Home-Manager 或 NixOS 配置：

```nix
# flake.nix（你的系统配置）
inputs.pulse-ring-nix.url = "github:yigexuanmu/pulse-ring-nix/neo";
```

```nix
# 在 home-manager / configuration.nix 里
{ pkgs, inputs, ... }: {
  home.packages = [
    inputs.pulse-ring-nix.packages.${pkgs.system}.pulse-ring
  ];
}
```

安装后的二进制自带 `--set PULSE_RING_ELECTRON` 指向 nix 提供的 Electron，无需你额外 `npm install`。

### 3. 仅开发 / 调试时

```bash
nix develop        # 进入 devShell（已注入 PKG_CONFIG_PATH / PULSE_RING_ELECTRON / LD_LIBRARY_PATH）
cargo run --release   # 直接跑源码
```

devShell 内 `PULSE_RING_ELECTRON` 已指向 nix 的 electron，`cargo run` 不需在 `electron-wallpaper/` 跑 `npm install`。

---

## 二、在配置里启用 Folia 歌词层

pulse-ring 的配置文件（QML 语法，默认 `~/.config/pulse-ring/pulse-ring.qml`）里加一行 **`scene_wallpaper`**：

```qml
// scene_wallpaper 是「常驻场景」，独立于壁纸轮播（不参与 rotation）
// 用相对名时先装壁纸包到壁纸库（见下），用绝对路径则直接指向 pack 目录
scene_wallpaper: "folia-lyrics"
```

### 把壁纸包装进壁纸库（推荐）

pulse-ring 的壁纸库目录是 `~/.config/pulse-ring/wallpapers/`。把仓库里的 pack 拷过去：

```bash
mkdir -p ~/.config/pulse-ring/wallpapers
cp -r assets/wallpapers/folia-lyrics ~/.config/pulse-ring/wallpapers/
```

之后配置里写裸名 `scene_wallpaper: "folia-lyrics"`，pulse-ring 会自动到壁纸库找。

> 内置 pack 通过相对路径 `../../../folia-wallpaper/dist/index.html` 指向已构建的 folia 页面。若你用 `nix build` 的安装产物，`postInstall` 已把 `folia-wallpaper/` 一并复制到 `$out/share/pulse-ring/`，相对路径仍能解析。

---

## 三、切换歌词可视化模式 ⭐

有三种方式，**优先级从高到低**：

### 方式 A：改壁纸包的 `project.json`（最常用）

编辑 `~/.config/pulse-ring/wallpapers/folia-lyrics/project.json` 的 `params.visualizerMode`：

```json
{
  "type": "scene",
  "title": "Folia 歌词可视化",
  "file": "../../../folia-wallpaper/dist/index.html",
  "audio": true,
  "resolution": "1920x1080",
  "params": { "visualizerMode": "monet" }
}
```

可选值（任一即可，非法值回退到 `classic`）：

| 模式 | 渲染技术 | 风格速写 |
|------|----------|----------|
| `classic` | Canvas 2D | 经典逐行高亮 |
| `cadenza` | Canvas 2D | 节奏卡点动效 |
| `partita` | DOM/framer-motion | 段落卡片式 |
| `fume` | DOM/framer-motion | 烟雾式弥散 |
| `claddagh` | DOM/framer-motion | 心形/菱形几何 |
| `cappella` | DOM + 头像贴图 | 虚拟歌姬头像 |
| `tilt` | DOM/framer-motion | 倾斜分屏 |
| `monet` | WebGPU/Three.js | **依赖封面取色**（见下）|
| `diorama` | WebGL/Three.js | 立体场景微缩 |
| `pendolo` | Canvas 2D | 摆钟式时间轴 |
| `sonnet` | WebGL/PixiJS | 杂志大片构图 |

改完重启 pulse-ring 生效（config 在窗口加载时推送）。

### 方式 B：URL 参数 `?mode=`（会话级锁定，覆盖方式 A）

直接给 `project.json` 的 `file` 字段加 query：

```json
"file": "../../../folia-wallpaper/dist/index.html?mode=sonnet"
```

`?mode=` 设定后**整个会话期间锁死**该模式，方式 A 的 config 不再覆盖它。适合「我要强行只跑这一个模式调试」。

### 方式 C：不设（默认 `classic`）

`params` 里没有 `visualizerMode`、URL 也没 `?mode=`，就回退到 `classic`。

---

## 四、monet 等依赖封面取色的模式

`monet` 模式从专辑封面提取主题配色（5 色调色板）。本集成已自动处理：

- pulse-ring 拿到 MPRIS 封面 (`playerctl metadata mpris:artUrl`)
- 后台线程缩到 256×256 → JPEG → base64 → `data:image/jpeg;base64,...`
- 经 playback 推送作 `coverUrl` 给 folia 页
- folia 页用 `new Image()` + canvas `getImageData` 提色（**data URL 无 CORS / canvas taint 问题**，Electron 默认 webSecurity 下也能直接加载）

所以**你不用做任何封面配置**，只要你的播放器在 MPRIS 上报了 `mpris:artUrl`，monet 就能随专辑变色。

---

## 五、数据来源（透明）

| folia 需要的数据 | pulse-ring 来源 | 备注 |
|------------------|-----------------|------|
| 歌词行 + 时间轴 | pulse-ring 自带的 `lyrics` 线程 | LRC / 增强 LRC，自动解析 |
| 播放进度时钟 | MPRIS `playerctl position` | folia 端客户端外推，2Hz 重锚 |
| 标题/艺术家/专辑 | MPRIS metadata | |
| 专辑封面 | `mpris:artUrl` | base64 data URL（见上）|
| 音频频段 | PipeWire monitor → FFT 128 频段 | folia 的 5-band + 整段频谱都驱动 |
| 主题配色 | pulse-ring 的 ring 颜色 + sensitivity | Rust 推 `theme` JSON |
| 翻译/和声 | LRC 带翻译时透传 | 普通 LRC 无和声 → 安全 fallback |

folia 端**不联网、不搜歌、不拉在线歌词**——pulse-ring 怎么取，folia 就怎么收。

---

## 六、重建 Folia 页面 bundle（可选）

仓库已带预构建的 `folia-wallpaper/dist/`（开箱即用）。**只有你修改了 `folia-wallpaper/src/` 下的源码**时才需重建：

```bash
cd folia-wallpaper
npm install        # 首次需要
npm run build      # 产物落到 folia-wallpaper/dist/
```

构建产物随仓库提交（非开发者无需装 node/npm），nix 纯沙箱构建**不会**在构建期跑 npm（已用 `doCheck = false` 跳过需字体的单测）。

---

## 七、故障排查

| 现象 | 排查 |
|------|------|
| 歌词层不显示 | 确认 `scene_wallpaper` 指向了 pack 且 pack 在壁纸库/绝对路径有效 |
| 只显示 `classic`，改 `params` 没用 | 升级到 `ebae0a2` 之后的提交（早期版本 `__FOLIA_MODE__` 在 Electron contextIsolation 下失效）|
| `Electron not found` | 安装产物被 wrap 过；检查 `PULSE_RING_ELECTRON` 环境变量或 `electron-wallpaper/node_modules/.bin/electron` 存在与否 |
| monet 模式不变色 | 确认播放器在 MPRIS 报了 `mpris:artUrl`（`playerctl metadata mpris:artUrl`）|
| nix build 失败 `npm not found` | 升级到 `2da8fd0` 之后的提交（已移除沙箱内 npm 重建）|

---

## 八、相关文件

- 壁纸包清单：[`assets/wallpapers/folia-lyrics/project.json`](assets/wallpapers/folia-lyrics/project.json)
- Folia 页面工程：[`folia-wallpaper/`](folia-wallpaper/)（入口 `src/PulseRingObsApp.tsx`）
- Rust 协议 / 序列化：[`src/folia_bridge.rs`](src/folia_bridge.rs)、[`src/web_wallpaper.rs`](src/web_wallpaper.rs)
- Electron 桥接：[`electron-wallpaper/main.js`](electron-wallpaper/main.js)、[`electron-wallpaper/preload.js`](electron-wallpaper/preload.js)
- 三层合成管线：[`src/draw.rs`](src/draw.rs)、[`src/main.rs`](src/main.rs)
- Nix 构建：[`flake.nix`](flake.nix)
