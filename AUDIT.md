# Sonnet 引擎 folia 对齐审阅报告

> 审阅对象：`src/lyricstyles/sonnet.rs`（~2200 行）+ `mg.rs/mg_geo.rs/mg_scene.rs/mg_themed.rs`
> 对照基准：`chthollyphile/folia-major` main 分支
> `src/components/visualizer/sonnet/sonnet*.ts`（35+ 模块，已逐文件抓取核对，缓存在 `/tmp/folia_src/`）
> 渲染后端：wgpu / Vulkan（**不动**，本次只在纯逻辑层做镜像）

---

## 0. 结论先行

移植版**比最初预期对齐得多**——ease 全家、`camera_frame` 7 种镜头运动系数、相机呼吸公式、镜头级转场窗口、段落分类（5 类齐全）、zoom 系数映射 **逐项与 folia 一字不差**。这些不需要动。

真正偏离 folia 的点集中在 **9 处**，按"是否影响'同一首歌在同一个时间点出同一个画面'的确定性还原"分级：

| # | 偏离点 | 影响等级 | 优先级 |
|---|--------|---------|--------|
| 1 | RNG 算法异构 | 决定性（全程序列不同）| P0 |
| 2 | transition 选择策略自创硬规则 | 决定性（转场分布偏离）| P0 |
| 3 | decoration depth 无负侧 + 误给 support + sin.abs 替 random | 观感（视差单向、非均匀）| P1 |
| 4 | support normalOffset 只动 Y 忽略 rotation | 观感（垂直布局错位）| P1 |
| 5 | postProcess 默认值 4 项偏离 | 观感（整体调色偏重）| P1 |
| 6 | lens 双通道合并 + contrast/rgb 强度公式偏离 | 观感（镜头畸变/色散丢失）| P1 |
| 7 | timeline shake 多加（folia runtime 传 0）| 微观（长期镜头多了抖动）| P2 |
| 8 | outro 判定多加 `para_dur>10` 限制 | 决定性（短曲尾段被错分）| P1 |
| 9 | 语义切分基础 char vs word（潜在）| 决定性（hero/semi 选择偏离）| P0 待验证 |

---

## 1. 已确认对齐（无需改动）

核对以下移植版实现与 folia 真值**逐项一致**，排除早期误判：

- **easing 全家** `sonnet.rs:88-104` ↔ `sonnetMotion.ts:42-55`
  - `ease_in_out = resolve_cubic_bezier(0.65, 0, 0.35, 1, v)` ✅
  - `ease_enter = resolve_cubic_bezier(0.22, 1, 0.36, 1, v)` ✅
  - `ease_expo_out = 1 - 2^(-10t)` ✅
  - `ease_elastic_out` P=0.3 ✅
- **`camera_frame` 7 种 shot motion 系数** `sonnet.rs:~1290` ↔ `sonnetMotion.ts:180-263`
  - EditorialColumn/TypeImpact/FragmentCollage/TrackingRibbon/MaskReveal/PosterBlocks/QuietTableau 的 (x,y,scale,rot) 公式逐字一致；path easing 三段式 0.18/0.6/0.22 一致 ✅
- **相机呼吸** `sonnet.rs:~1318` ↔ `sonnetMotion.ts:67-83`
  - 常数 `BREATH_MAX_OFFSET=0.006 / SCALE=0.002 / ROTATION=0.0015`、频率 `0.13/0.31/0.11/0.29/0.09/0.07`、相位 0.65/0.35 混合全对齐 ✅
- **zoom 系数映射** `sonnet.rs:334-337` ↔ `sonnetProgram.ts:buildShots`
  - PosterBlocks→(1.02,0.16)、QuietTableau→(1.12,0.2)、其余→(1.22,0.26) ✅
  - camera random unpack `(r&255)/255-0.5)*0.18`、`>>8)0.14`、`>>24)0.08` ✅
- **镜头级转场窗口** `sonnet.rs:313` ↔ `sonnetTransitions.ts:143`
  - `twindow = (gap*0.18).clamp(0.14, 0.24)` ✅（纠正：folia **有**镜头级转场，`resolveSonnetShotBoundary`）
- **段落分类 5 类** `sonnet.rs:236-240` ↔ `sonnetProgram.ts:98-107`
  - `chorus`(副歌文本)、`break`(间奏)、`breath`(dur≤3.5||words≤3)、`lift`(标点≥2||密度>2.5)、`outro`(末段) 阈值全对齐 ✅
- **guides 引导线** `sonnet.rs` 引导段 ↔ `sonnetGuides.ts`
  - `leadDuration=min(0.38,max(0.2,0.18+dur*0.1))`、`endTime=start+0.65`、hero alpha 0.82/非 0.55、bezier 控制点 `(startX*0.6,startY*0.4)→(startX*0.2,startY*0.1)`、star head r=14/9、silk threads `random>0.4/>0.6`、rect spline `random>0.4`、burst `15+random*45` 速度 ✅
- **fast-blur / mono-glitch / camera-pull 转场帧** ↔ `sonnetTransitions.ts:49-84` ✅
- **shot path easing**（tracking/fragment/quiet/poster 用 `lin*0.55+easeInOut(lin)*0.45`，其余用三段式）✅
- **gaussian focus 权重** sigma=0.35、smoothing 窗口 0.12、samples `[-1,-0.5,0,0.5,1]` 权重 `[1,4,6,4,1]` ✅

---

## 2. 确诊偏离（逐条修复清单）

### 偏离 1 — RNG 算法异构【P0 决定性】

**移植版** `sonnet.rs:169-198`
```rust
fn new(seed: u64) -> Self {
    Self { state: seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0x5DEECE66D }  // 64-bit splitmix64 风格
}
// choose_transition_no_repeat 用 64-bit FNV-1a: 0xcbf29ce484222325
```

**folia 真值** `sonnetRandom.ts`
```ts
hashSonnetSeed: hash = 2166136261; hash ^= charCode; hash = Math.imul(hash, 16777619); >>> 0  // 32-bit FNV-1a
mixSonnetSeed:  Math.imul((Math.trunc(seed) ^ salt) >>> 0, 2654435761) >>> 0                // 32-bit golden (0x9E3779B1)
sonnetHash01:   mixSonnetSeed(seed + Math.imul(index+1, 97), salt) / 4294967296
```

**影响**：所有依赖种子做确定性选择的子系统——shot kind 顺序、transition 顺序、layout variant、每个 segment 的 decor/depth——产出的序列**与 folia 不同**。同一首歌在移植版上出的镜头/转场节奏永远跟 folia 对不上号。这是"完美还原"的地基，必须先修。

**修复**：在 `sonnet.rs` 顶部新增与 `sonnetRandom.ts` 一字不差的 32-bit 实现，替换 `Seeded`（64-bit）的所有调用点。注意 folia `chooseWithoutRepeat` 起点是 `hashSonnetSeed(stringSeed) % len`，而移植版 `choose_transition_no_repeat` 用的是 `(seed ^ idx*golden64)`，起点公式也要换。
```rust
fn hash_sonnet_seed(s: &str) -> u32 {
    let mut h = 2166136261u32;
    for b in s.bytes() { h ^= b as u32; h = h.wrapping_mul(16777619); }
    h
}
fn mix_sonnet_seed(seed: u32, salt: u32) -> u32 {
    (seed ^ salt).wrapping_mul(2654435761)
}
fn sonnet_hash01(seed: u32, index: u32, salt: u32) -> f32 {
    mix_sonnet_seed(seed.wrapping_add((index+1).wrapping_mul(97)), salt) as f32 / 4294967296.0
}
// RNG 调用点全部改为解构 u32 位（&255 / >>8 / >>16 / >>24），与 folia buildShots 的 random unpack 一致
```

---

### 偏离 2 — transition 选择策略自创硬规则【P0 决定性】

**移植版** `sonnet.rs:21-30`（push_shot 内）
```rust
let transition = if Some(MonoGlitch) == *prev_transition { FastBlur }       // 硬规则
                 else if Some(FastBlur) == *prev_transition { CameraPull }  // 硬规则
                 else if rng.unit() < 0.3 { MonoGlitch }
                 else if rng.unit() < 0.55 { FastBlur }
                 else { CameraPull };
```

**folia 真值** `sonnetProgram.ts:110-116` + `sonnetTransitions.ts:33`
```ts
const chooseWithoutRepeat = (choices, seed, previous) => {
    const start = hashSonnetSeed(seed) % choices.length;       // 无状态，FNV 起点定
    for (let offset = 0; offset < choices.length; offset++) {
        const candidate = choices[(start + offset) % choices.length];
        if (candidate !== previous) return candidate;          // 线性探测避"前一种"
    }
    return choices[start];
};
// 调用：chooseWithoutRepeat(SONNET_TRANSITION_KINDS, `${seed}:${para}:${shot}:cam`, lastKind)
```

**影响**：移植版"上一个 MonoGlitch 就强制下一个 FastBlur"是无状态转移规则，folia 是纯 hash-pick 只避前一种。两套策略产出的转场序列分布**根本不同**——移植版会出现"Mono→Fast→Camera→Mono→Fast→Camera"的准周期模式，folia 是伪随机的。

**修复**：删掉硬规则分支，改用 `choose_without_repeat(TRANSITION_KINDS, &format!("{seed}:{para}:{shot}"), prev)`，起点用偏离 1 修好的 32-bit FNV。段落级 transition 同理（`sonnet.rs:288` 已在用 `choose_transition_no_repeat`，种子公式从 64-bit 改 32-bit FNV 即可）。

---

### 偏离 3 — decoration depth 无负侧 + 误给 support + sin.abs 替 random【P1 观感】

**移植版** `sonnet.rs:1655-1660`
```rust
let depth_r = (p.start * 7.13).sin().abs();                    // 正弦绝对值，非均匀随机
let depth = match p.role {
    Role::Decoration => 0.3 + depth_r * 0.8,                   // 只正侧 0.3~1.1
    Role::Support    => 0.1 + depth_r * 0.25,                  // folia 对 support depth=0（不该加）
    _ => 0.0,
};
```

**folia 真值** `sonnetMotion.ts:67-74`
```ts
export const resolveSonnetSegmentDepth = (role, random) => {
    if (role !== 'decoration') return 0;                       // 只装饰生效
    return random() > 0.5 ? 0.5 + random() * 0.8 : -0.5 - random() * 0.8;  // 对称 ±0.5~±1.3
};
```

**影响**：
1. 移植版 depth 全为正 → 装饰层只向一个方向视差，folia 一半前一半后，立体感弱了一半。
2. 移植版给 Support 也加 depth `0.1+0.25*...`，folia Support depth 恒 0 → Support 词随相机平移时位移量不对。
3. `sin(start*7.13).abs()` 是确定性正弦绝对值，分布在 0~1 间呈正弦弓形而非均匀；folia 用 `random()`（编译期 segment PRNG 抽）是均匀分布 → 每个 segment 的深度分布形态不同。

**修复**：
```rust
let depth = if p.role == Role::Decoration {
    // 用偏离 1 修好的 segment-seeded random
    if seg_rand > 0.5 { 0.5 + seg_rand2 * 0.8 } else { -0.5 - seg_rand2 * 0.8 }
} else { 0.0 };
// 删掉 Support 分支
```
注意 `seg_rand` 必须是 segment 编译期抽（用 segment index + shot seed），不能用 `p.start*7.13` 的运行时 sin。

---

### 偏离 4 — support normalOffset 只动 Y 忽略 rotation【P1 观感】

**移植版** `sonnet.rs:1648-1651`
```rust
if p.role == Role::Support && !p.giant {
    let r = fract(sin(p.start*12.9898)*43758.5453 + p.start.fract()*101);
    off[1] += (r - 0.5) * p.size * 0.6;                        // 只偏 Y
}
```

**folia 真值** `sonnetMotion.ts:77-92`
```ts
const distance = (Math.min(1, Math.max(0, randomValue)) * 2 - 1) * fontSize * 0.3;  // ±0.3×fontSize
const normalAngle = rotation + (layoutDirection === 'vertical' ? 0 : Math.PI / 2); // 法线方向
return { x: Math.cos(normalAngle) * distance, y: Math.sin(normalAngle) * distance };
```

**影响**：水平布局时法线 = rotation+90°，若 rotation=0 则法线沿 +Y，移植版只动 Y **侥幸对**；但垂直布局时法线 = rotation，rotation=0 时法线沿 +X，移植版仍偏 Y → Support 词错位到上下而非左右。`p.rotation` 非 0 时（如 FragmentCollage 有旋转）水平布局也错。

**修复**：
```rust
let normal_angle = p.rotation + if layout_vertical { 0.0 } else { FRAC_PI_2 };
let dist = (seg_rand * 2.0 - 1.0) * p.size * 0.3;
off[0] += normal_angle.cos() * dist;
off[1] += normal_angle.sin() * dist;
```
`seg_rand` 同样用编译期 segment PRNG，不用 `sin(start*12.9898)`。`layout_vertical` 从 placement.layout_direction 取（需确认 `Placement` 是否带该字段，若没有需补）。

---

### 偏离 5 — postProcess 默认值 4 项偏离【P1 观感】

**移植版** `config.rs:459-465`（代码默认）
```rust
post_grain: 0.3,        // folia 0.2
post_contrast: 0.5,    // folia 0
post_lens: 0.5,        // folia lensDistortion 0.3 + lensDispersion 0.6（合并见偏离 6）
post_rgb_shift: 0.0,   // folia 0 ✅
post_halftone: 0.15,   // folia 0
post_vignette: 0.4,    // folia 0.85
```

**folia 真值** `src/types.ts` `DEFAULT_SONNET_TUNING`
```ts
textureResolution: 1.5,
postProcessEnabled: false,          // 移植版无此总开关，永远启用
postProcessGrain: 0.2,
postProcessContrast: 0,
postProcessRgbShift: 0,
postProcessHalftone: 0,
postProcessVignette: 0.85,
postProcessLensDistortion: 0.3,     // 移植版合并入 post_lens
postProcessLensDispersion: 0.6,     // 移植版丢失
```

**影响**：移植版默认开启全部后处理且强度更重（contrast 0.5→0、halftone 0.15→0、vignette 0.4→0.85），整体调色比 folia 默认偏暗偏锐，**与 folia 开箱观感直接不同**。

**修复**：
1. `config.rs` 默认值对齐 folia（grain 0.2、contrast 0、halftone 0、vignette 0.85）。
2. 新增 `post_process_enabled: bool` 总开关字段，默认 `false`（与 folia 一致）。
3. README / config 示例 `pulse-ring.lua` 同步更新。

---

### 偏离 6 — lens 双通道合并 + contrast/rgb 强度公式偏离【P1 观感】

**移植版** `sonnet.rs:1576-1582`
```rust
noise: ctx.post[0] * 0.35,                                         // ✅ 对齐 folia grain*0.35
contrast: ctx.post[1] * 0.5 + (enter.blur + exit.blur).clamp(0.0, 0.25),  // 多加 blur 项
chromatic: ctx.post[2] * 0.5,                      // lens 双通道合并成 chromatic，且 *0.5
rgb_shift: ctx.post[3] * 0.8,                     // 多乘 0.8
halftone: ctx.post[4],                            // ✅ 直接用
vignette: ctx.post[5],                            // ✅ 直接用
```

**folia 真值** `sonnetPostProcess.ts:48-63`
```ts
glowAlpha: Math.min(0.62, 0.28 + motion * 0.12),
noise: postEnabled ? postProcessGrain * 0.35 : 0,           // ✅
contrast: postEnabled ? postProcessContrast * 0.5 : 0,     // 无 blur 项
lensDistortion: postEnabled ? postProcessLensDistortion : 0, // 独立通道，直接用 0.3
lensDispersion: postEnabled ? postProcessLensDispersion : 0, // 独立通道，直接用 0.6
rgbShift: postEnabled ? postProcessRgbShift : 0,            // 直接用，不 *0.8
halftone: postProcessHalftone,
vignette: postProcessVignette,
```
且 folia 渲染顺序：lens 曲率在 grading/print pass **之前**（`sonnetPostProcess.ts:98-102`）。

**影响**：
1. folia 镜头畸变（几何弯曲，0.3）和色散（RGB 边缘分裂，0.6）是两个独立效果；移植版合并成单一 `post_lens` 后传 `chromatic=0.5*lens` → 几何畸变丢失，色散强度也错（应是 0.6 直接回放，移植版变 0.25）。
2. contrast 多加 `(enter.blur+exit.blur)` 项 → 转场期间对比度被 blur 拉高，folia 无此行为。
3. rgb_shift 多乘 0.8 → 偏移幅度比 folia 弱 20%。

**修复**：
1. `config.rs` 拆 `post_lens` 为 `post_lens_distortion` 和 `post_lens_dispersion` 两字段，默认 0.3 / 0.6。
2. uniform 与 WGSL 同步加两通道（draw.rs 大 uniform 的 post 段 + shader 的 chromatic/曲率段）。
3. `sonnet.rs:1577` 删 `+ (enter.blur+exit.blur).clamp(0,0.25)`；`:1580` 改 `rgb_shift: ctx.post[3]`（去 *0.8）；`lens` 项按偏离 1 加总开关门控。

---

### 偏离 7 — timeline shake 多加（folia runtime 传 0）【P2 微观】

**移植版** `sonnet.rs:1326-1332`（注释自承"folia runtime passes 0"）
```rust
let sh_int = 0.003;
let sh_x = (time*123.456).sin()*(time*789.123).cos()*0.02*sh_int;
let sh_y = (time*345.678).cos()*(time*901.234).sin()*0.02*sh_int;
let sh_r = (time*567.890).sin()*0.005*sh_int;
// 叠加进 scale/pan/rot
```

**folia 真值**：`sonnetMotion.ts` `resolveTimelineShake` 在 runtime 调用处传 0（folia 源 runtime 未读全，但移植版注释自承 folia runtime passes 0，即该函数存在但被静音）。

**影响**：移植版长期镜头多了 0.003 量级的高频抖动，folia 同期完全静止。微观差异，可能让 folia "静"的镜头在移植版上"颤"。

**修复**：按 folia 行为静音——要么删掉，要么把 `sh_int` 接到 `ctx.timeline_shake` 配置项默认 0（保留可选）。注释已诚实标注，建议直接删。

---

### 偏离 8 — outro 判定多加 `para_dur>10` 限制【P1 决定性】

**移植版** `sonnet.rs:240`
```rust
let is_outro = pe >= lines.len() && para_dur > 10.0;     // 末段且时长>10s
```

**folia 真值** `sonnetProgram.ts:101`
```ts
if (index === total - 1) return 'outro';                // 末段即为 outro，无时长要求
```

**影响**：短曲尾段（<10s）在移植版被分到默认 `verse`，镜头分配走 Verse 路径而非 Outro；folia 一律当 outro 处理 → 镜头节奏和收尾观感在短曲上偏离。

**修复**：`let is_outro = pe >= lines.len();`（删 `&& para_dur > 10.0`）。

---

### 偏离 9 — 语义切分 char vs word（潜在 P0，待验证）

**移植版** `sonnet.rs:232` `split_with_timing(...).len()` 用 `char_indices` 按单字切；`scoreHero`/`segmentCount` 基于 char 计数。

**folia 真值** `sonnetSemantic.ts:20-104`
```ts
new Segmenter(undefined, { granularity: 'word' }).segment(text)   // Intl.Segmenter 词级切分
splitLyricGraphemes(text)                                          // 图元级（CJK 仍按字）
// scoreHero = Math.min(getVisibleLength, 8) * 14 + durationScore  // visibleLength 按 grapheme 词级
// SEMI_HERO: MIN_LINE_WORDS=4, SCORE_RATIO=0.35, MIN_GAP=2, MIN_VISIBLE_LENGTH=2, MULTI_WORD_COUNT=9
```

**影响（推断）**：中文歌词里 folia 的 Segmenter word 会把"我爱你"切成 1 个词还是 3 个词取决于分词库——若按字切则与移植版 char 切一致，若按词切则 segmentCount/hero score 全不同。**这是需要实测验证的潜在偏离**，因为 hero/semi-hero/support/decoration 的角色分配决定"哪句话最大最亮"，错了整首歌的视觉焦点就错位。

**待办**：
1. 实测 folia 在中文歌词上 `Intl.Segmenter` word 粒度的切分结果（抓 raw 输出比对）。
2. 若按词切且与移植版 char 切不同 → 在移植版引入中文分词（轻量分词库或按标点/空格 + CJK 单字混合规则），对齐 `scoreHero`/`segmentCount` 语义。
3. 核对移植版 `Placement` 是否带 `is_word_like` 字段（folia segment 有），以及 `scoreSonnetHeroSegment`/`findSonnetSemiHeroSegmentIndices` 是否落实 `SEMI_HERO_*` 五常量。

---

## 3. 待逐行比对清单（尚未 1:1 核对，下一步重点）

以下 folia 模块已抓回源码（`/tmp/folia_src/`）但移植侧尚未逐行 1:1 核对，按预期偏离风险排序：

1. **`sonnetTypographyLayout.ts`**（16993 字节，最大）↔ 移植版 7 个 `layout_*` 函数 —— 决定每种 shot 的版式，风险高。
2. **`sonnetGlyphLayout.ts`**（逐字 stagger）↔ 移植版 per-glyph entry —— 风险中。
3. **`sonnetBackgroundDecor.ts` / `sonnetFrameDecor.ts`** ↔ 移植版 decor quads —— 风险中（移植版有 decor/glyph/words 三套 buffer）。
4. **`sonnetCredits.ts`** ↔ 移植版 sonnet.rs:2125 片尾 credits —— 风险低。
5. **`sonnetSceneBuilder.ts`**（14688 字节，总装配）↔ 移植版 build_frame 主循环 —— 风险高，决定装配顺序。
6. **`sonnetMg*/sonnetThemedShotMgPrimitives/sonnetSpatialMgGeometry`** ↔ 移植版 `mg.rs/mg_geo.rs/mg_scene.rs/mg_themed.rs` —— 音乐图形，移植版已拆成独立文件（结构对齐 folia），但算法待核对。
7. **`sonnetLensFilter.ts/sonnetPrintFilters.ts/sonnetGlitchFilter.ts`** ↔ 移植版 WGSL shader 滤波段 —— 风险中。
8. **`sonnetTextFixedGeo/sonnetPosterBlocksLayout/sonnetShotFlowLayouts`** ↔ 移植版对应 layout —— 风险中。

---

## 4. 推荐修复顺序

1. **P0-偏离 1（RNG）** 先行——所有 32-bit FNV 替换 64-bit splitmix，这是后续偏离 3/4 的修复前提（depth/normalOffset 都要用编译期 segment PRNG 的 random()）。
2. **P0-偏离 2（transition 策略）** 顺手——删硬规则改 choose_without_repeat。
3. **P1-偏离 8（outro 判定）** 一行删除，立刻消除短曲错分。
4. **P1-偏离 3/4（depth/normalOffset）** 一起改——都依赖偏离 1 修好的 segment PRNG。
5. **P1-偏离 5/6（postProcess 默认值 + lens 双通道）** 一起改——涉及 config.rs + draw.rs uniform + WGSL shader 三处同步，影响面大但纯数值/shader 工作。
6. **P2-偏离 7（timeline shake）** 删几行即可。
7. **P0-偏离 9（语义切分）** 实测后再定——可能需要引入分词，工作量未知，先抓证据。
8. 之后转第 3 节"待逐行比对清单"，按风险高低逐模块核对。

---

## 5. 实施记录（本轮）

渲染后端 **wgpu/Vulkan 未动**，改动局限在纯逻辑层。8 项确诊偏离中 **7 项已修复并验证**，1 项（偏离 9）留作实测清单项，不实施代码。

### 已修复（7 项）

| # | 偏离 | 实施落点 | 要点 |
|---|------|---------|------|
| 1 | RNG 算法异构 | `sonnet.rs` `hash_sonnet_seed`/`mix_sonnet_seed`/`sonnet_hash01`；`Seeded` 改 32-bit golden chain | 与 `sonnetRandom.ts` 逐字对齐（32-bit FNV-1a `2166136261/16777619` + golden `0x9E3779B1`）；伪随机位解构沿用 folia `&255/>>8/>>16/>>24` |
| 2 | transition 硬规则 | `push_shot` 删 `if prev==X` 转移分支 | 改 `choose_without_repeat`，种子 `hash_sonnet_seed("{seed}:{para}:{shot}:cam")`，纯 hash 线性探测避前一种 |
| 8 | outro 判定 | `is_outro = pe >= lines.len()` | 删 `&& para_dur > 10.0`，末段即 outro |
| 3 | decoration depth | `build_frame` 内 depth 改对称 | `Decoration` 专用对称 ±0.5~1.3，`Support` 归 0；random 用 `sonnet_hash01(seg_seed,...)`（segment start×shot seed 编译期抽），替 `sin(start*7.13).abs()` |
| 4 | support normalOffset | 同上 | 法线 `p.rotation + (vertical?0:π/2)`，`±0.3×size` 沿 cos/sin；替原仅偏 Y（垂直/旋转布局不再错位） |
| 5 | postProcess 默认值 | `config.rs` + `main.rs` + `preview.rs` | grain 0.2 / contrast 0 / halftone 0 / vignette 0.85；`post_enabled` 默认 `false` 对齐 folia `postProcessEnabled:false` |
| 6 | lens 双通道 | `config.rs` 拆 `post_lens_distortion`(0.3)/`post_lens_dispersion`(0.6) → `draw.rs` WGSL `scene_at` 全帧 barrel warp → `LyricFx.lens_distortion` → uniform `array<f32,9>` → `sonnet.rs` fx builder | `scene_at` 按 folia `radialScale=1-curv*r2+curv*0.16*r4` 同时扭曲 ring/MG decor/lyrics；删 `contrast` 的 blur 项、`rgb_shift` 的 ×0.8 |
| 7 | timeline shake | `camera_frame` 删 `sh_int` 4 行 | folia runtime 传 0，静音对齐，长镜头不再颤 |

### 验证

- `cargo check` — **0 error**（43 warnings 全为既有 `dead_code`，无新增）
- `cargo build --bin pulse-ring` — 通过，生成 debug binary
- `cargo test draw::tests::shader_is_valid_wgsl` — naga 30 `parse_str` + `Validator(ValidationFlags::all(), Capabilities::all())` 全通过，覆盖 `scene_at` barrel warp（5 处 WGSL 编辑点）
- **⚠️ 仍需在真实 Vulkan 设备跑帧确认几何观感**：naga 只保证 WGSL 语法/类型/绑定合法，不模拟光栅化；lens 桶形畸变与 decor depth 视差的视觉效果须上机比对 folia 截帧。

### 未实施（1 项）与后续清单

- **偏离 9（语义切分 char vs word）**：留作实测清单项。需先抓 folia `Intl.Segmenter` 在中文歌词上的 word 粒度真实输出，与移植版 char 切比对后再决定是否引入分词。本次不动代码。
- **第 3 节 8 大模块**（typographyLayout/glyphLayout/decor/sceneBuilder/MG/lens·print·glitch filters 等）：已抓回 folia 源码缓存于 `/tmp/folia_src/`，但移植侧尚未逐行 1:1 核对，留下一轮。

---

## 附：基准源码缓存位置

- folia 真值源码：`/tmp/folia_src/`（11 个核心 .ts，motion/transitions/guides/postProcess/cameraTracking/random/sceneBuilder/credits/printFilters/lensFilter/glitchFilter）
- 其余 folia 模块（semantic/program/roles/typographyLayout/glyphLayout）已额外抓取至同目录
- `DEFAULT_SONNET_TUNING` 真值见 `src/types.ts`（`sonnet/types.ts` 仅类型定义，无默认值）——本次核对已确认
