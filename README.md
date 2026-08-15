# pulse-ring-nix

Wayland 壁纸层上的音乐律动可视化 + 全屏歌词引擎（GPU 渲染，wgpu/Vulkan）。

本仓库在 [MEKCCK/pulse-ring](https://github.com/MEKCCK/pulse-ring) 圆环律动的基础上，新增了完整的 **Sonnet（商籁）电影化歌词引擎**——对 [folia-major](https://github.com/chthollyphile/folia-major) 的 PV 风格歌词动画的忠实移植：语义角色、七套排版模板、逐字动力学入场、镜头运镜、转场与后处理，全部程序化实时渲染（无图片素材）。

---

## 架构总览

```
┌────────────────────────────────────────────────────────────┐
│  Lua 脚本层（可选）                                          │
│  粒子 / 音频幅度 / 动态调参 / 频段变换（pulse.* API）        │
└───────────────┬────────────────────────────────────────────┘
┌───────────────▼────────────────────────────────────────────┐
│  Rust 内核（每帧）                                          │
│  ├─ 音频   audio.rs      PipeWire/ALSA monitor → FFT → 128 频段
│  ├─ 元数据 main.rs       playerctl (MPRIS) → 曲目/封面/进度
│  ├─ 歌词   lyrics.rs     内嵌 lyricfetch → LyricData（9 源）
│  ├─ 动画   lyricstyles/  sonnet.rs 引擎 → Vec<CharQuad>
│  ├─ 图集   sdf.rs        fontdue → SDF 四档字重 glyph atlas
│  └─ 渲染   draw.rs       wgpu(WGSL) → wl-layer-shell
└────────────────────────────────────────────────────────────┘
```

**分层原则**：CPU 每帧用纯时间 + 确定性种子计算出所有动画（拖动进度条与连续播放画面一致）；GPU 只做图集采样与后处理。

---

## 源码地图（src/）

| 文件 | 行数 | 职责 |
|---|---|---|
| `main.rs` | ~1600 | 入口：Wayland layer-shell 会话、wgpu 初始化、MPRIS 轮询、封面解码上传、歌词 worker 调度、主循环 |
| `config.rs` | ~1270 | QML 样式解析（`Config`）、`LyricStyle`、`pulse-ring sonnet true\|false` CLI、MG/后处理开关 |
| `draw.rs` | ~1860 | wgpu 渲染器 + 内嵌 WGSL shader：脉冲环、widgets、歌词层、MG 装饰、全部后处理 |
| `audio.rs` | 300 | cpal 采集（PipeWire monitor，`pactl` 解析 `PIPEWIRE_NODE`）、实时 FFT、128 对数频段 |
| `lyrics.rs` | 124 | 歌词抓取 worker（内嵌 Rust `lyricfetch` 模块）、统一行模型、LRCLIB 兜底 |
| `lyricview.rs` | 655 | 歌词渲染共享核心：`CharQuad`（每字 20 个 f32）、SDF 绘制原语、`StyleCtx/StyleInput`、相机变换 |
| `sdf.rs` | 416 | 四档字重（regular/bold/black/light）SDF 图集：光栅化 → 有向距离场 → 打包 |
| `lyricstyles/` | ~3900 | 动画样式；`sonnet.rs` 是主体，`mg*.rs` 是 MG 装饰背景 |
| `preview.rs` | 333 | 无 GPU 预览：CPU 模拟 shader 数学输出 PNG |
| `lua.rs` / `plugin.rs` | ~700 | Lua 行为层 / C ABI 动态插件 |
| `lyricfetch/` | ~3150 | 内嵌多源 Rust 歌词适配器（9 源：LRCLIB/NetEase/QQ/Kugou/SPlayer/QiShui/TTML/Spotify/Apple/Musixmatch），in-process HTTP，无子进程 |

---

## 渲染管线（draw.rs）

### CharQuad —— 歌词层的唯一图元

每个字符（或 MG 线段/矩形/三角形）是 20 个 f32，写入 GPU storage buffer（容量 3276 quad）：

```
[slot, uv(4), px(2), pos(2), scale, alpha, rotate, color(4), ext(4)]
```

`slot`（glow 字段）同时作为**哨兵**分派：

| slot | 含义 |
|---|---|
| `0..1` | SDF 字形（glow 强度） |
| `252` | 填充三角形（MG 装饰，顶点在 `ext`/`uv`） |
| `254` | 圆角矩形 / 线段（设 `rotate` 为线段角） |
| `255` | 圆角胶囊（`lw==lh` 即精确圆） |

### WGSL shader 结构

```
fs_main → scene_at(p)
   ├── ring_at(p)        环/魔法阵/粒子/widgets（可被 RGB shift 二次采样）
   └── 歌词层循环         逐像素 × 全部 quad：
        ├ 粗排除（轴对齐距离）→ 旋转 → 槽位分派
        ├ 字形：SDF 采样 + blur(×14) + glow + CA + glitch(双带/亮度撕裂)
        ├ MG：矩形/三角形 SDF
        └ 合成：MG 先、文字后（alpha-over）
   后处理（全屏）：镜头畸变、RGB shift、CMYK halftone、vignette、noise、contrast
```

RGB shift 只对 `ring_at` 二次采样（25° 轴 ±1.25px），歌词循环保持单次，避免性能翻倍。

### 性能要点

- 歌词层每像素 × quad 数是最主要成本：`QUAD_BUDGET`（sonnet.rs）限制单帧 quad ≤ 200，文本优先、超载只裁 MG 装饰尾部
- MG 场景（`MgScene`）按 seed+shot 缓存（`MG_CACHE`），每镜头只构建一次
- 曲线细分自适应（圆 12 段、弧 /30、二次 6、三次 10）

---

## Sonnet 歌词引擎（lyricstyles/sonnet.rs）

folia `sonnet` 的忠实移植（约 2000 行），逐模块对应：

| folia 源文件 | 本仓库实现 |
|---|---|
| `sonnetProgram.ts` | `compile_program`：段落（间隙阈值/6行/18s）→ 镜头（≤4行/6s，7 模板不重复）→ 段落分类（chorus/breath/lift/outro） |
| `sonnetSemantic.ts` | `split_with_timing`（标点粘附、逐字时间戳） |
| `sonnetTypographyRoles.ts` | 得分制 hero + semi-hero（阈值/gap/对侧回退/双 semi） |
| `sonnetTypographyLayout.ts` | 模板特化字号（editorial 4.0/1.2 … tableau 3.0/1.15）、CJK 竖排、非 CJK 旋转 90°、hero 巨字背景副本、82% 预 fit + 7 档全局 fit |
| `sonnetShotFlowLayouts.ts` | 7 套 layout（editorial 5 / tableau 4 / ribbon 3 / collage 3 variants） |
| `sonnetMotion.ts` | cubic-bezier 求解、逐字形 0.65–1.8s settle、镜头路径、呼吸 ramp、timeline shake、Gaussian 焦点权重 |
| `sonnetTransitions.ts` | fast-blur / mono-glitch / camera-pull（镜头级 + 段落级窗口） |
| `sonnetGlyphLayout.ts` | 逐字交替 ±24% 偏移 + 旋转入场（`char_fly`） |
| `sonnetTextViewBuilder.ts` | 色差 CA、semi-hero 幽灵残影、guide 曲线/丝线/星头/形状爆发、rectSpline |
| `sonnetGuides.ts` | 贝塞尔尾迹 + 头部光点 + 概率丝线 + 矩形样条 + 形状爆发 |
| `sonnetShotMg*.ts` | `mg.rs/mg_geo.rs/mg_themed.rs/mg_scene.rs`：48 几何 variant、HUD、固定几何、粒子、扫描线 |
| `sonnetPostProcess.ts` | 镜头畸变 / RGB shift / CMYK halftone / vignette / noise / contrast |
| `sonnetSceneBuilder.ts` | `[SONNET]` 主题标签、扫描线背景 |
| `sonnetCredits.ts` | 片尾曲目海报 + outro 模糊 |
| 器乐模式 | `virtual_staff()`：无歌词时生成 ♪ 谱线 |

### 数据流

```
LyricLine[] → compile_program (段落/镜头)
  → build_placements (角色/字号/测量/巨字)
  → layout_* (7 模板，变体由文本 seed 决定)
  → 逐帧 build_frame：
       per-char fly → push_word_full → CharQuad
       + MgScene（缓存）+ guide + 字幕 + credits
  → apply_camera_local (镜头/焦点/漂移/转场)
  → set_lyrics → WGSL
```

所有随机量由 `ctx.seed`（曲目 hash）确定性派生；动画仅依赖绝对时间，seek 安全。

---

## 音频链路（audio.rs）

```
PipeWire/ALSA monitor（pactl 解析 PIPEWIRE_NODE）
  → cpal 采集（F32/48kHz/1024 缓冲）
  → 汉宁窗 + realfft（2048 窗口）
  → 128 对数频段（40Hz–16kHz，快升慢降衰减）
  → 环形律动 + sonnet 粒子音频（bass×0.34+vocal×0.52+power×0.14 → 指数平滑）
```

无音频设备时回落到"呼吸模式"（`silent_source`）。

---

## 歌词获取（lyrics.rs + 内嵌 lyricfetch 模块）

- `LyricWorker` 后台线程：曲目变化 → 内嵌 Rust `lyricfetch` 模块（9 源：LRCLIB/NetEase/QQ/Kugou/SPlayer/QiShui/TTML/Spotify/Apple/Musixmatch）→ `LyricData`
- 行模型 `LyricLine`：start_ms / duration_ms / text / translation / romanization / 逐字 chars 时间戳
- 自动 LRCLIB 兜底；无匹配时启用虚拟 ♪ 谱线

---

## 配置

### QML —— 静态样式（`~/.config/pulse-ring/pulse-ring.qml`）

```qml
PulseRing {
    style: "sonnet"          // off | sonnet（也可用 CLI 切换）
    shape: "ring"
    colors: ["#6750A4", "#7D5260", "#D0BCFF", "#EADDFF"]
    // 歌词层开关与后处理参数
    showBackgroundMg: true   // MG 背景（HUD/几何/扫描线）
    showFixedGeo: true       // 固定几何
    showBackgroundDecor: true// 粒子
    postProcessEnabled: true // 后处理总开关
    postGrain: 0.3           // 噪点
    postContrast: 0.5        // 对比度
    postLens: 0.5            // 镜头畸变
    postRgbShift: 0.0        // RGB shift（全屏，关=默认以保性能）
    postHalftone: 0.15       // CMYK 网点
    postVignette: 0.4        // 暗角
    fontWeight: 0            // 手动字重 300/400/700/900（0=按角色自动）
    widgets: [ Widget { type: "cover"; x: 0.82; y: 0.16; size: 0.14 } ]
}
```

### Lua —— 动态行为（`pulse-ring.lua`）

`onUpdate(dt)` / `transformBands(bands)` / `pulse.*` API 控制粒子、幅度、参数。

### CLI

```bash
pulse-ring sonnet            # 查看当前状态
pulse-ring sonnet true       # 启用商籁歌词
pulse-ring sonnet false      # 关闭（保留圆环）
pulse-ring preview "文本" sonnet 12.0 out.png   # 无 GPU 预览
```

---

## 构建与运行

```bash
# Nix
nix run github:yigexuanmu/pulse-ring-nix
nix develop -c cargo build   # 开发环境（含 alsa/wayland/xkbcommon）

# Arch/manual
cargo build --release
```

**systemd 用户服务**（依赖：pactl、playerctl 需在 PATH）：

```bash
systemctl --user start pulse-ring.service    # 运行
systemctl --user stop pulse-ring.service     # 关闭
journalctl --user -u pulse-ring.service -f   # 日志
```

首次运行自动生成 `~/.config/pulse-ring/pulse-ring.qml` + `pulse-ring.lua`。

---

## 调试

| 环境变量 | 作用 |
|---|---|
| `PULSE_RING_DEBUG_PREVIEW` | 打印每帧 shot/placement/quad 明细 |
| `PULSE_RING_CAPTURE=/path.png` | 首帧截图 |
| `PULSE_RING_MG_VARIANT=N` | 强制指定 MG 几何 variant（0–47） |
| `PULSE_RING_DUMP_GLYPH` | ASCII 转储字形 SDF |

测试：`cargo test`（含 WGSL 合法性校验、SDF 图集、MG 全 variant 构建）。

---

## 许可证

GPL-3.0-or-later，详见 [LICENSE](LICENSE)。歌词引擎移植自 folia（AGPL-3.0），仅供学习与技术交流。
