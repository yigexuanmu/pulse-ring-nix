# pulse-ring 配置 GUI 设计文档 / Settings GUI Design

> GTK4 + libadwaita，独立进程，中英双语。
> 负责两块配置：① 上游 pulse-ring 主环/壁纸/挂件（`pulse-ring.qml`）；② folia 歌词可视化 11 模式调参（`folia-lyrics.json`）。
> Independent GTK4 process, bilingual (zh/en). Edits both upstream QML config and folia lyric-visualizer tuning.

---

## 1. 目标 / Goals

- 不动 Rust 运行时渲染逻辑：GUI 只读写磁盘配置文件，pulse-ring 重启或 SIGHUP 后生效。
- 上游 50+ 配置字段全覆盖（形状/颜色/三层环/光环/粒子/壁纸/挂件/音频/生成动画）。
- folia 11 模式 Tuning 全字段覆盖（抄 `folia/settingsPanels.tsx` 的字段与范围）。
- GTK4 原生控件（SpinButton/Scale/Switch/ColorButton/DropDown/SpinRow），libadwaita 视觉。
- 中文/English 一键切换，即时生效，记住上次选择（`~/.config/pulse-ring/gui.json`）。

## 2. 架构 / Architecture

```
┌─────────────────────────┐        ┌──────────────────────────┐
│  pulse-ring-config (GUI)│        │  pulse-ring (wallpaper)   │
│  GTK4 + libadwaita      │        │  Rust/Wayland/wgpu        │
│                         │ 读写   │                           │
│  读 ~/.config/pulse-ring│───────▶│  启动时读同一批配置文件    │
│  /pulse-ring.qml        │  磁盘   │  → 渲染                   │
│  /folia-lyrics.json     │        │                           │
│  /gui.json (语言偏好)   │        │  folia_bridge 读 json     │
└─────────────────────────┘        │  → send_config 给 Electron│
        │ SIGHUP (可选)            │  → folia 页面应用 tuning   │
        └──────────────────────────▶└──────────────────────────┘
```

**关键决策 / Key decisions:**

1. **独立进程**：GUI 不链接 wgpu/wayland/gstreamer，二进制小、启动快、不抢壁纸层焦点。
2. **磁盘交换**：GUI 与 pulse-ring 只通过文件通信，无 IPC。改完提示用户"重启 pulse-ring 生效"（或对 wallpaper 进程发 SIGHUP——若上游支持热重载则启用，否则提示）。
3. **folia tuning 存独立 JSON**：`~/.config/pulse-ring/folia-lyrics.json`，不污染 QML（符合解耦原则：歌词层独立于壁纸）。结构：
   ```json
   { "mode": "sonnet", "tuning": { "classic": {...}, "partita": {...}, ... } }
   ```
   存全 11 模式的 tuning（切模式保留各模式旋钮值）。`mode` 即启用哪个可视化模式。
4. **Rust 侧最小改动**：仅 `folia_bridge.rs` spawn 时多读 `folia-lyrics.json`，把 `{"visualizerMode":..., "foliaTuning":...}` 经 `send_config` 推给 Electron 页面。这是对"我自己的 folia 子系统"的扩展，不碰上游壁纸运行时。
5. **folia 页面侧最小改动**：`PulseRingObsApp` 的 `applyConfig` 读 `cfg.foliaTuning[mode]`，作为该模式 Tuning 的初始值传给 `VisualizerRenderer`。

## 3. 窗口结构 / Window layout

libadwaita `AdwPreferencesWindow` + 多 `AdwPreferencesPage`：

| Page (zh) | Page (en) | 内容 |
|-----------|-----------|------|
| 形状与颜色 | Shape & Color | shape/corners/spikiness/rotate/colorMode/colors/ringWidth/baseRadius/growth/halo/alpha |
| 三层环 | Three Rings | innerRing/innerRadius/innerGrowth/innerWidth/innerColor/innerAlpha；midRing/midRadius/midGrowth/midWidth/midColor/outerUniform；saturnBand/Alpha/Stripes |
| 生成动画 | Spawn | spawnEffect⚠️/spawnDuration/spawnEase/spawnRotate⚠️（标注解析器缺陷，见 §7） |
| 粒子 | Particles | particleShape/particleMode/particleLoop/particles[] 列表编辑 |
| 壁纸 | Wallpaper | imageWallpaper/mode；videoWallpaper/audio；webWallpaper；sceneWallpaper；wallpapers[] 轮播列表+interval/transition/effect(50种下拉)；luaScript |
| 挂件 | Widgets | widgets[] 列表编辑（type/x/y/size/alpha/rotate/color/fontSize/...按类型分组） |
| 音频与位置 | Audio & Position | sensitivity/decay/smoothness/idleBreathe/xOffset/yOffset/renderScale/renderScreen |
| **歌词可视化** | **Lyric Visualizer** | folia 11 模式选择 + 当前模式 Tuning 字段（从 folia `settingsPanels.tsx` 抄） |
| 语言 | Language | 中文 / English 切换 |

⚠️ = 已确认的上游解析器缺陷，GUI 中以警告标识并据实禁用或标注。

## 4. 上游字段映射表 / Upstream field map

QML 键名 → 控件类型 → 范围（来自 `config.rs` 默认值与 clamp）：

| QML key | 控件 | 范围/选项 | 默认 |
|---------|------|-----------|------|
| shape | DropDown | ring/square/diamond/hexagon/triangle/star/flower | ring |
| corners | Scale | 2–20 | 5 |
| spikiness | Scale | 0–1 | 0.35 |
| rotate | Scale | -180–180° | 0 |
| autoRotate | Scale | -30–30°/s | 4 |
| colorMode | DropDown | hue/solid/gradient | gradient |
| colors | 颜色数组编辑 | RGBA hex 列表 | MD3 紫 |
| color | ColorButton | hex | — |
| ringWidth | Spin | 0.5–30 | 7 |
| baseRadius | Scale | 0.02–0.5 | 0.13 |
| growth | Scale | 0–0.5 | 0.20 |
| haloStrength | Scale | 0–1 | 0.18 |
| haloSize | Scale | 0–0.5 | 0.12 |
| alpha | Scale | 0–1 | 1 |
| innerRing | Switch | bool | true |
| innerRadius | Scale | 0.1–0.95 | 0.58 |
| innerGrowth | Scale | 0–0.5 | 0.08 |
| innerWidth | Spin | 0.5–20 | 5 |
| innerColor | ColorButton | hex | #EADDFF |
| innerAlpha | Scale | 0–1 | 0.9 |
| midRing | Switch | bool | true |
| midRadius | Scale | 0.1–0.95 | 0.78 |
| midGrowth | Scale | 0–0.5 | 0.08 |
| midWidth | Spin | 0.5–20 | 3.5 |
| midColor | ColorButton | hex | #938F99 |
| outerUniform | Switch | bool | false |
| saturnBand | Scale | 0–0.2 | 0.022 |
| saturnAlpha | Scale | 0–1 | 0.22 |
| saturnStripes | Scale | 0–1 | 0.35 |
| dashCount | Spin | 0–64 | 0 |
| dashRatio | Scale | 0–1 | 0.8 |
| renderScale | Scale | 0.25–1 | 1 |
| renderScreen | Spin | -1–7 | -1 |
| spawnEffect⚠️ | DropDown | none/expand/zoom/**magic(失效)** | magic⚠️ |
| spawnDuration | Spin | 200–5000 ms | 1400 |
| spawnEase | DropDown | outCubic/outBack/elastic/bounce | outCubic |
| spawnRotate⚠️ | Spin | -360–360° | 0(不受理解析) |
| particleShape | DropDown | circle/square/diamond/star | circle |
| particleMode | DropDown | burst/orbit/ring/none | burst |
| particleLoop | Switch | bool | true |
| particles | 列表编辑 | x/y/angle/speed/size/life/delay/gravity/drag/fadeIn/sizeEnd/twinkle/wave/spinSpeed/color | [] |
| imageWallpaper | FileChooser | 路径 | — |
| imageWallpaperMode | DropDown | cover/contain/stretch | cover |
| videoWallpaper | FileChooser | 路径 | — |
| videoWallpaperAudio | Switch | bool | true |
| webWallpaper | FileChooser | html 路径 | — |
| sceneWallpaper | FileChooser | 文件夹/html | — |
| wallpapers | 列表编辑 | 路径数组+增删排序 | [] |
| wallpaperInterval | Spin | 5–3600 s | 30 |
| wallpaperTransition | Scale | 0.1–10 s | 1.2 |
| wallpaperTransitionEffect | DropDown | 50 种 (Fade…ZoomInCircles) | fade |
| luaScript | FileChooser | .lua | 默认 |
| sensitivity | Scale | 0.1–5 | 1.4 |
| decay | Scale | 0.5–0.99 | 0.86 |
| smoothness | Scale | 0–8 | 2 |
| idleBreathe | Scale | 0–0.5 | 0.05 |
| xOffset/yOffset | Scale | -0.5–0.5 | 0 |
| widgets | 列表编辑 | 按类型（ring/image/clock/bars/cover/analog/plugin/lyric）分组字段 | [] |

## 5. folia 11 模式 Tuning 映射 / Folia tuning map

字段全集来自 `folia/types.ts` 的 `DEFAULT_*_TUNING`，范围来自 `settingsPanels.tsx` 的 clamp。控件按类型：Switch(bool)/Scale(f32, 带 clamp)/DropDown(enum)。

| 模式 | 字段 (类型) |
|------|-------------|
| **classic** | enableWordRotation(Switch) · breathingFloatMultiplier(Scale 0–2) · useLegacyLayout(Switch) · wordSpacing(Scale 0–2) |
| **partita** | showGuideLines(Switch) · useSemanticLayout(Switch) · staggerMin(Scale 0–180) · staggerMax(Scale 0–180) |
| **fume** | hidePrintSymbols(Switch) · disableGeometricBackground(Switch) · backgroundObjectOpacity(Scale 0–1) · textHoldRatio(Scale 0–1) · cameraTrackingMode(DropDown smooth/…) · cameraSpeed(Scale) · glowIntensity(Scale) · heroScale(Scale) |
| **claddagh** | focusScaleRatio(Scale 0–1.5) · radiusScale(Scale 0.5–1.5) · ellipseTiltDeg(Scale 0–60) · showAxisLine(Switch) · letterSpacingOffset(Scale) |
| **cappella** | showEmoMessages(Switch) · emojiPackSource(DropDown) · avatarSource(DropDown cover/…) |
| **tilt** | splitProbability(Scale 0–1) · tiltStyleProbability(Scale 0–1) · colorScheme(DropDown) |
| **diorama** | cameraSpeed · motionAmount · audioReactivity · particleDensity(5–1500) · particleScale · particleGlowEnabled · particleGlowIntensity · showParticles · backgroundParticleCircumference · backgroundParticleRadial · glowEnabled · glowIntensity · soulEnabled · soulIntensity · soulActiveEnabled · gradientEnabled · gradientIntensity · keywordColoringEnabled · geometryVisibility(子对象) |
| **cadenza** | fontScale · widthRatio · motionAmount · glowIntensity · beamIntensity |
| **monet** | keywordColoringEnabled · showDescription · audioStyle(DropDown bar/…) · fontScale · portraitSource(DropDown) · portraitOffsetX · portraitStyle(DropDown) · showPortraitDragHanger |
| **pendolo** | arcRadius · arcAngleDeg · wheelCenterX · wheelCenterY · tickSnappiness · activeScale · showGearDecor(DropDown) · showCenterGradient(Switch) · showCoverOnWatchFace(Switch) · enableLineGlow(Switch) |
| **sonnet** | cameraIntensity · typographyMotion · mgDensity · showOnlyText · showGuide · showBackgroundMg · showFixedGeo · showGiantDecorativeText · showBackgroundDecor · enableTransitions · outerFrameMode(DropDown none/frame/full) · textureResolution · postProcessEnabled · postProcess{Grain,Contrast,RgbShift,Halftone,Vignette,LensDistortion,LensDispersion} |

标签文案来自 `folia/i18n/locales/{en,zh-CN}.ts` 的 `options.*` 命名空间（如 `classicWordRotation` = "逐字旋转"/"Per-word Rotation"）。

## 6. i18n 双语机制 / Bilingual i18n

- GUI 独立翻译表，不依赖 folia 的 i18next（那是 Electron 页面用的）。
- `i18n.rs`（或 `i18n` 模块）：内嵌 `zh` / `en` 两个 `HashMap<&str,&str>`，键用 folia 既有 `options.*` 键名 + GUI 专有键（`ui.tab.shape` 等）。
- `gui.json` 存 `{ "lang": "zh" | "en" }`，启动读，切换即写、即时刷新（重渲染所有标签）。
- 系统语言作初始默认（`LANG` 含 `zh` → zh，否则 en）。

## 7. 已确认的上游解析器缺陷 / Confirmed upstream parser bugs

GUI 据@FindBy实标注，不掩盖：

1. **`spawnEffect: "magic"`（默认值）被解析器映射为 `Expand`** —— `SpawnEffect::Magic` 变体成死代码。GUI 选项里 "magic" 标⚠️并提示"当前上游解析器不生效，将退化为 expand"。
2. **`spawnRotate` 键解析器不处理** —— 始终 0。GUI 该字段标⚠️"当前上游忽略此值"。

（GUI 只诚实标注，不动 Rust 解析器。修不修上游另议。）

## 8. 工程结构 / Project layout

```
src/config_gui/          # GTK4 Rust GUI（独立 bin: pulse-ring-config）
  main.rs                # 入口
  qml_io.rs              # 读写 pulse-ring.qml（复用 config.rs 的 parse，新增 serialize 回 QML）
  folia_json.rs          # 读写 folia-lyrics.json
  i18n.rs                # 双语表
  pages/                 # 各 AdwPreferencesPage
    shape.rs three_rings.rs spawn.rs particles.rs
    wallpaper.rs widgets.rs audio.rs folia.rs language.rs
gui.json                 # (运行时生成于 ~/.config/pulse-ring/)
```

Cargo.toml：新增 `[[bin]] name="pulse-ring-config"`；deps 加 `gtk4`、`libadwaita`（glib 已间接有）。

## 9. 生效方式 / Apply changes

- 保存→写盘→弹 toast "已保存，重启 pulse-ring 生效"。
- 可选：若 pulse-ring 进程在跑，GUI 提供"重启 pulse-ring"按钮（杀进程 + 重新 `nix run`/systemd 启动）。
- 不做 live IPC（YAGNI；上游本就重启生效，GUI 不引入新机制）。

## 10. 风险 / Risks

- **QML 序列化**：`config.rs` 只有 parse，无 serialize。需在 GUI 侧写一个 QML writer（键值→QML 文本，保留注释较难——v1 丢注释，生成新文件）。或复用 AST：parse→edit→print。v1 采用"全量重写"（读入 Config→用户改→输出完整 QML），简单可靠。
- **字典序/注释丢失**：全量重写会丢原文件注释。v1 接受（默认 qml 注释可再生），或保留"未识别键"。后续可升级 AST 编辑。
- **widget/particle 列表编辑**：UI 复杂，v1 支持增删改基础字段，复杂类型（各 widget 不同字段）用分组表单。
