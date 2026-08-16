# Sonnet 引擎编译器级 1:1 重写实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 将 folia 的 sonnet 可视化引擎（55 个 TS/TSX 文件 ≈ 10853 行 + 外部 `@chenglou/pretext` 排版库 ≈ 5279 行核心源）以编译器级语义等价移植为 Rust，字形像素对 FreeType 参考输出 byte-identical，替换现有 `src/lyricstyles/sonnet.rs`（2381 行挪用式实现）。

**架构：** X 形态——持久化场景图 + arena 整数索引（`SonnetSceneArena{scenes,shots,segments,glyphs,ghosts,mg_layers,guides,frame_decors}`），`render_frame(t)` 字面 mutate arena 字段（映射 PixiJS `view.alpha=...`/`position.set(...)`），帧末 `flatten` 产 `Vec<CharQuad>` 喂现有 `draw.rs`/WGSL `scene_at`（零改动）。执行 worktree：`/tmp/sonnet-rewrite`，分支 `feat/sonnet-1to1-rewrite`，基线 `connet@c644f4c`。

**技术栈：** Rust 2021 edition · nix devShell（`nix develop`，flaked）· `cargo` · FreeType（glyph coverage 光栅，via `freetype` crate）+ harfbuzz（shaping，via `harfbuzz_rs`）· `unicode-segmentation`（UAX#29 词/字素簇切分，替代 `Intl.Segmenter`）· `unicode-bidi`（Bidi 算法）· 字形光栅替换 `fontdue` SDF → FreeType coverage 直存 atlas（G1 方案）。

**复刻判据：**
- 纯算法层（motion/semantic/program/random/camera/transition/grapheme-timing/render-hints）：逐行译，浮点 literal 分毫不差，TS `Math.imul`/`>>> 0` → Rust `u32::wrapping_mul`/`as u32`。
- 排版层：pretext 内部算法逐函数译（`Intl.Segmenter` → `unicode-segmentation`；`Canvas.measureText` → FreeType advance；跳过仅 DOM emoji 校正——Rust atlas 无 emoji）。
- arena/场景层：PixiJS 对象图（`Container`/`Sprite`/`Graphics`/`Text`）→ Rust arena node + mutate-then-flatten。
- 字形像素：FreeType (`FT_RENDER_MODE_NORMAL` coverage, 同 ppem + load_flag) 输出 byte-identical 于 Foliation 参考光栅；pulse-ring 浏览器 canvas DTO 不做比对（颜色管线不同）。
- snapshot 验证：9 个 `snapshot_eq_*.txt` 文件，Rust 端串行化 arena + compiler 产物、与 folia 端 mirror dump 对照。

---

## 决策锁定（不再询问用户）

| 维度 | 决策 | 理由 |
|---|---|---|
| 架构形态 | **X**（持久化 arena + mutate-then-flatten） | 字面满足"绝不 Rust 风格优化、不架构重构、Arena+索引"硬约束 |
| 范围 | **55 folia 文件全移植** | 用户明确"全 Rust"——UI/React 落 QML `StyleCtx` 开关、SVG icon 落 Rust polylines、TexturePool/Debug 落 arena runtime helpers、不在 Rust 省略功能 |
| pretext | **A 字面移植**（5279 行）| 用户要"编译器 1:1、绝不 Rust 风格简化"；B 等价 API 不符。仅底层不可移植 API 用 Rust 替代 |
| 字形 | **G1**（FreeType coverage 直存 atlas + 改写 `draw.rs` `scene_at` glyph 取样段）| folia 字形本质是 coverage 不是 SDF；"连字形也要等" 落 FreeType 参考光栅 byte-identity |
| 执行模型 | **主会话亲自逐文件**——不再 fan-out | 本会话 fan-out 四种死法全撞过：402 余额 / 0-tool-uses+502 / concurrency-limit / length-截断 |
| 验证 | **snapshot 对拍 + cargo check + cargo test + FreeType 光栅对照** | 字面实现 writing-plans 的 TDD + verification-before-completion |

---

## 文件结构

### 创建（Rust 新文件，全部位于 `/tmp/sonnet-rewrite/src/lyricstyles/sonnet_v2/` 子模块）

新模块树（编译器级隔离——旧 `sonnet.rs` 保留直到 Phase 9 切换）：

```
src/lyricstyles/sonnet_v2/mod.rs              — 子模块入口 + pub fn build_frame 委派
src/lyricstyles/sonnet_v2/types.rs            — 公共契约（SonnetParagraph/Shot/ShotKind/Program/Segment/...）
src/lyricstyles/sonnet_v2/random.rs           — hashSonnetSeed/mixSonnetSeed/sonnetHash01
src/lyricstyles/sonnet_v2/semantic.rs         — buildSonnetSemanticSegments（替代 Intl.Segmenter via unicode-segmentation）
src/lyricstyles/sonnet_v2/grapheme_timing.rs  — splitLyricGraphemes/buildLineGraphemeTimeline（移植 graphemeTiming.ts）
src/lyricstyles/sonnet_v2/render_hints.rs     — getLineRenderEndTime/buildLineRenderHints（移植 renderHints.ts 243 行）
src/lyricstyles/sonnet_v2/program.rs          — compileSonnetProgram/findSonnetParagraphIndexAtTime（移植 sonnetProgram.ts）
src/lyricstyles/sonnet_v2/motion.rs           — 全部 motion/camera/breath/shake（移植 sonnetMotion.ts 282 行）
src/lyricstyles/sonnet_v2/camera.rs           — resolveSonnetCameraTrackingGlyphs/SegmentCameraFocus（移植 sonnetCameraTracking.ts）
src/lyricstyles/sonnet_v2/transitions.rs     — resolveSonnetTransitionEffectFrame/Exit/Enter/Shot（移植 sonnetTransitions.ts）
src/lyricstyles/sonnet_v2/typography_roles.rs — scoreSonnetHeroSegment/find*HeroSegmentIndex/resolveRoleFontWeight
src/lyricstyles/sonnet_v2/typography_layout.rs— resolveSonnetTypographyLayout（移植 sonnetTypographyLayout.ts 404 行）
src/lyricstyles/sonnet_v2/glyph_layout.rs    — buildSonnetGlyphLayout（移植 sonnetGlyphLayout.ts）
src/lyricstyles/sonnet_v2/shot_flow_layouts.rs— 所有 layout* 函数（移植 sonnetShotFlowLayouts.ts 557 行）
src/lyricstyles/sonnet_v2/poster_blocks.rs   — layoutSonnetPosterBlocks（移植 sonnetPosterBlocksLayout.ts 331 行）
src/lyricstyles/sonnet_v2/arena.rs            — SonnetSceneArena + 所有 ID/newtype + flatten
src/lyricstyles/sonnet_v2/scene_builder.rs    — buildSonnetScene: arena 构造（移植 sonnetSceneBuilder.ts 346 行）
src/lyricstyles/sonnet_v2/text_view_builder.rs— buildSonnetTextView: glyph arena 节点构造（移植 sonnetTextViewBuilder.ts 420 行）
src/lyricstyles/sonnet_v2/guides.rs           — createSonnetGuide + guide geometry
src/lyricstyles/sonnet_v2/frame_decor.rs      — buildSonnetFrameDecor + resolveSonnetFrameDecorSpec
src/lyricstyles/sonnet_v2/credits.rs           — buildSonnetCreditsPoster + resolveSonnetCreditsFrame
src/lyricstyles/sonnet_v2/post_process.rs     — 每帧 uniform amount/seed 计算（移植 sonnetPostProcess.ts）
src/lyricstyles/sonnet_v2/filters.rs          — openGL filter source（移植 3 个 filter TS 文件的 GLSL → 复用 draw.rs 现有 WGSL）
src/lyricstyles/sonnet_v2/animations.rs       — AnimatedGraphics 对等物（矢量笔法累计）
src/lyricstyles/sonnet_v2/staff_view.rs       — buildSonnetStaffView（器乐模式）
src/lyricstyles/sonnet_v2/staff_notation.rs   — staff notation 笔法
src/lyricstyles/sonnet_v2/runtime.rs           — SonnetPixiRuntime 对等物 = arena + render_frame + carryover 状态机
src/lyricstyles/sonnet_v2/mg/mod.rs           — MG family 调度
src/lyricstyles/sonnet_v2/mg/shot_mg.rs       — 中央 dispatcher (sonnetShotMg.ts 814 行)
src/lyricstyles/sonnet_v2/mg/marine.rs        — sonnetShotMgMarine
src/lyricstyles/sonnet_v2/mg/craft.rs         — sonnetShotMgCraft
src/lyricstyles/sonnet_v2/mg/celestial.rs      — sonnetShotMgCelestial
src/lyricstyles/sonnet_v2/mg/kinetic.rs       — sonnetShotMgKinetic
src/lyricstyles/sonnet_v2/mg/music.rs         — sonnetShotMgMusic
src/lyricstyles/sonnet_v2/mg/architecture.rs   — sonnetShotMgArchitecture
src/lyricstyles/sonnet_v2/mg/landscape.rs     — sonnetShotMgLandscape
src/lyricstyles/sonnet_v2/mg/flora.rs         — sonnetShotMgFlora
src/lyricstyles/sonnet_v2/mg/botanical.rs     — sonnetShotMgBotanical
src/lyricstyles/sonnet_v2/mg/themed.rs        — sonnetThemedShotMg + Primitives
src/lyricstyles/sonnet_v2/mg/extended.rs       — sonnetExtendedShotMg
src/lyricstyles/sonnet_v2/mg/additional.rs    — sonnetAdditionalShotMg
src/lyricstyles/sonnet_v2/mg/background.rs    — sonnetBackgroundMgVariants + sonnetBackgroundDecor
src/lyricstyles/sonnet_v2/mg/open_frame.rs    — sonnetOpenFrameShotMg
src/lyricstyles/sonnet_v2/mg/fixed_geo.rs      — sonnetFixedGeoVariants + sonnetTextFixedGeo
src/lyricstyles/sonnet_v2/mg/spatial.rs        — sonnetSpatialMgGeometry
src/lyricstyles/sonnet_v2/pretext/mod.rs      — pretext Rust 移植入口（前 patch）
src/lyricstyles/sonnet_v2/pretext/layout.rs   — prepareWithSegments/layoutWithLines/measureText 等（移植 pretext layout.ts）
src/lyricstyles/sonnet_v2/pretext/analysis.rs — analyzeText/kinsoku/segment 分块（移植 analysis.ts 1458 行）
src/lyricstyles/sonnet_v2/pretext/line_break.rs— line-break 算法（移植 line-break.ts 1236 行）
src/lyricstyles/sonnet_v2/pretext/measurement.rs— advance 测量（移植 measurement.ts 275 行，用 FreeType）
src/lyricstyles/sonnet_v2/pretext/bidi.rs     — Bidi 算法（移植 bidi.ts 175 行）
src/lyricstyles/sonnet_v2/pretext/bidi_data.rs — Unicode bidi 数据（生成数据 dump，移植 bidi-data.ts 996 行）
src/lyricstyles/sonnet_v2/pretext/line_text.rs— 辅助（移植 line-text.ts 107 行）
src/lyricstyles/sonnet_v2/pretext/rich_inline.rs— 移植 rich-inline.ts 518 行
```

### 创建（工具/契约新文件）

```
src/lyricstyles/sonnet_v2/freeglue.rs          — FreeType 集成 + coverage atlas（替换 sdf.rs 部分功能，独立模块）
tests/sonnet_v2/snapshot_eq_*.txt             — 9 个 snapshot 对照数据（与 folia mirror dump 同格式）
tests/sonnet_v2/mod.rs                        — 测试入口
src/lyricstyles/sonnet_v2/runtime_state.rs    — 对等 createSonnetPixiRuntime 状态字段（outro_blur 等 carryover）
```

### 修改

- `Cargo.toml` — 加 `freetype`, `harfbuzz_rs`(或 `harfbuzz-sys`), `unicode-segmentation`, `unicode-bidi`。
- `flake.nix` — devShell 加 `pkgs.freetype`, `pkgs.harfbuzz`, `pkgs.freetype.dev`（pkg-config）。
- `src/lyrics.rs:17` — `LyricLine` 加字段 `words: Vec<LyricWord>`, `syllables`（直接 via words）, `end_sec`, `song_part`, `block_index`, `is_chorus`(改名 `chorus_flag`)。`LyricLine.duration_ms` 改名为 `end_ms` 保持现有接口但新增 `end_sec()`。
- `src/lyricfetch/*.rs` — 同步填充新字段（多数源已含 wordsTiming/syllables，在 jsonparse.rs 已采集）。
- `src/draw.rs` `scene_at` WGSL fragment 段 ~150-200 行（`lyric_fx` + glyph SDF 取样）—— 改写为 coverage 直取（G1）。
- `src/sdf.rs` — `GlyphAtlas::spawn` 改用 FreeType 进行 coverage 光栅；保留 `struct Layout` 兼容 but `atlas_bytes` 改 coverage u8。
- `src/lyricstyles/mod.rs:Sonnet` arm 切换到 `sonnet_v2::build_frame`（Phase 9 最后一步）。
- `src/main.rs:1327` translation 处理链无需改（接口不变）。

---

## Phase 划分（9 个 Phase，每 Phase 多个任务）

执行 worktree：`cd /tmp/sonnet-rewrite`（所有命令基于此 cwd）。
每任务 = `编写测试 → 运行确认失败 → 实现 → 运行确认通过 → commit`。
编译器判据：`nix develop -c cargo check -p pulse-ring` 每 Phase 末必须 0 error。

---

## Phase 0：工具链与依赖（建 freetype/harfbuzz 工具链）

**目标：** 让 devShell 暴露 FreeType + harfbuzz，Cargo.toml 加四个 crate，跑通一次 `cargo check`。

### 任务 0.1：flake.nix devShell 加 freetype/harfbuzz

**文件：**
- 修改：`flake.nix:105-` mkShell packages 列表

- [ ] **步骤 1：读 flake.nix 确认 packages 块结构**

运行：`rg -n "pkgs\\.|mkShell|packages" /tmp/sonnet-rewrite/flake.nix | head -30`

- [ ] **步骤 2：在 packages 列表加三行**

```nix
            pkgs.freetype
            pkgs.harfbuzz
            pkgs.freetype.dev  # 提供 freetype.pc 给 pkg-config
            pkgs.harfbuzz.dev  # 提供 harfbuzz.pc
```

插在现有 `pkgs.libxkbcommon` 之后。

- [ ] **步骤 3：进 devShell 验证 .pc 可见**

运行：`nix develop -c bash -c 'pkg-config --modversion freetype2 harfbuzz'`
预期：打印两个版本号（无错）

- [ ] **步骤 4：commit**

```bash
git add flake.nix
git commit -m "build: expose freetype+harfbuzz in devShell for sonnet v2 glyph port"
```

### 任务 0.2：Cargo.toml 加四个 crate + 声明 sonnet_v2 模块占位

**文件：**
- 修改：`Cargo.toml`（dependencies 段）
- 创建：`src/lyricstyles/sonnet_v2/mod.rs`（仅 `pub fn build_frame` 占位 + 模块骨架）
- 修改：`src/lyricstyles/mod.rs`（暂不切换，先 `mod sonnet_v2;` 但 dispatch 仍走旧 `sonnet::build_frame`）

- [ ] **步骤 1：Cargo.toml 加依赖**

```toml
freetype = "0.7"
harfbuzz_rs = "2.0"
unicode-segmentation = "1.11"
unicode-bidi = "0.3"
```

- [ ] **步骤 2：建 sonnet_v2/mod.rs 骨架**

```rust
//! Folia sonnet engine, compiler-grade 1:1 port from TS.
//! See docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md.

pub mod types;

/// Entry — identical signature to crate::lyricstyles::sonnet::build_frame.
/// Not wired until Phase 11; the old `sonnet::build_frame` stays dispatch.
pub fn build_frame(
    _ctx: &crate::lyricview::StyleCtx,
    _input: &crate::lyricview::StyleInput,
) -> crate::lyricview::StyleOutput {
    crate::lyricview::StyleOutput::empty()
}
```

- [ ] **步骤 3：mod.rs 暂入但不切 dispatch**

`src/lyricstyles/mod.rs` 加 `pub mod sonnet_v2;`，但 `build_frame` match arm 仍走旧 `sonnet::build_frame`（Phase 11 才切）。

- [ ] **步骤 4：cargo check 通过**

运行：`nix develop -c cargo check -p pulse-ring 2>&1 | tail -5`
预期：`Finished` 0 error（warning 不计）

- [ ] **步骤 5：commit**

```bash
git add Cargo.toml src/lyricstyles/sonnet_v2/mod.rs src/lyricstyles/mod.rs
git commit -m "build: scaffold sonnet_v2 module + freetype/harfbuzz/unicode deps"
```

---

## Phase 1：扩充歌词数据契约（LyricLine 加 words/syllables/endTime/songPart）

**目标：** 让 Rust `LyricLine` 字段覆盖 folia `Line` 接口，并把 end_sec = ms/1000 暴露；改最小不伤旧 sonnet。

### 任务 1.1：LyricLine/Word/Syllable 结构扩字段

**文件：**
- 修改：`src/lyrics.rs:17-28` — `LyricLine` + 新增 `LyricWord`/`LyricSyllable` 结构

- [ ] **步骤 1：编写失败测试**

`tests/lyric_line_contract.rs`:
```rust
use pulse_ring::lyrics::{LyricLine, LyricWord};

#[test]
fn lyric_line_has_words_and_song_part() {
    let line = LyricLine {
        start_ms: 1000, duration_ms: 2000,
        text: "hello".into(), translation: "你好".into(), romanization: "".into(),
        chars: vec![],
        words: vec![LyricWord {
            text: "hello".into(),
            start_ms: 1000, end_ms: 3000,
            syllables: vec![],
        }],
        song_part: "verse".into(),
        block_index: 0,
        chorus_flag: false,
    };
    assert_eq!(line.end_sec(), 3.0_f32);
    assert_eq!(line.words[0].text, "hello");
}
```

- [ ] **步骤 2：运行验证失败**

```bash
cargo test --test lyric_line_contract 2>&1 | tail -5
```
预期：编译失败（字段 words/song_part 未定义）

- [ ] **步骤 3：扩字段**

```rust
#[derive(Debug, Clone)]
pub struct LyricSyllable {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LyricWord {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub syllables: Vec<LyricSyllable>,
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub start_ms: i64,
    pub duration_ms: i64,
    pub text: String,
    pub translation: String,
    pub romanization: String,
    pub chars: Vec<i64>,
    /// Per-word timing (parser-derived; may be empty → uniform split).
    pub words: Vec<LyricWord>,
    /// Song section tag ("verse"/"chorus"/"bridge"/…).
    pub song_part: String,
    /// Block index (metadata boundary detection).
    pub block_index: i64,
    /// Chorus flag (parsed from source or detected).
    pub chorus_flag: bool,
}

impl LyricLine {
    pub fn end_sec(&self) -> f32 {
        (self.start_ms + self.duration_ms) as f32 / 1000.0
    }
    pub fn start_sec(&self) -> f32 { self.start_ms as f32 / 1000.0 }
    pub fn end_ms(&self) -> i64 { self.start_ms + self.duration_ms }
}
```

- [ ] **步骤 4：跑所有旧构造点 cargo check，补缺字段为默认**

运行：`cargo check -p pulse-ring 2>&1 | grep -c 'missing field'`
预期间隔迭代至 0。所有旧 `LyricLine { ... }` 字面构造点（`preview.rs:45`, `main.rs`, `lyricfetch/*.rs`）补 `words: vec![], song_part: String::new(), block_index: 0, chorus_flag: false`。

- [ ] **步骤 5：跑测试通过**

```bash
cargo test --test lyric_line_contract 2>&1 | tail -3
```
预期：PASS

- [ ] **步骤 6：commit**

```bash
git add src/lyrics.rs tests/lyric_line_contract.rs src/preview.rs src/main.rs src/lyricfetch/
git commit -m "feat(lyrics): expand LyricLine contract (words/songPart/blockIndex/isChorus) for folia sonnet port"
```

### 任务 1.2：lyricfetch 收集器填充 words/syllables（jsonparse.rs 已采集）

**文件：**
- 修改：`src/lyricfetch/jsonparse.rs:161-` `for chkey in &["chars","charTimes","syllables","wordsTiming"]` 块

- [ ] **步骤 1：阅读现状 jsonparse.rs 已采集的 wordTiming/syllable 数据格式**

运行：`rg -n "syllables|wordsTiming|charTimes|chars" /tmp/sonnet-rewrite/src/lyricfetch/jsonparse.rs | head -20`

- [ ] **步骤 2：编写失败测试**

`tests/lyricfetch_words_test.rs` — 解析一个含 `wordsTiming` 与 `syllables` 的 JSON 样例，断言 `line.words[0].syllables.len() >= 1` 且 `words[0].text == "hello"`。

- [ ] **步骤 3：实现 words/syllable 提取**

在 `jsonparse.rs` 处理 `wordsTiming` 分支时填充 `words: Vec<LyricWord>`，每个 word 的 `syllables` 从 `syllables` 数组按 word index 映射。

- [ ] **步骤 4：测试通过 + cargo check**

- [ ] **步骤 5：commit**

```bash
git add src/lyricfetch/jsonparse.rs tests/lyricfetch_words_test.rs
git commit -m "feat(lyricfetch): populate words/syllables for sources exposing wordTiming"
```

---

## Phase 2：pretext 纯文本算法移植（A 字面方案，5279 行 → Rust）

**目标：** Rust 端实现 `prepare(text,font)→PreparedTextWithSegments`、`layoutWithLines(prepared,maxWidth,lineHeight)→LayoutLinesResult` 等 14 个 export，供 Phase 4 typography 层调用。`Intl.Segmenter` → `unicode-segmentation`；`Canvas measureText` → FreeType advance；跳过 DOM emoji DOM 校正（Rust atlas 无 emoji）。

### 任务 2.1：bidi 数据层（996 行生成数据）

**文件：**
- 创建：`src/lyricstyles/sonnet_v2/pretext/bidi_data.rs`

- [ ] **步骤 1：从 pretext `generated/bidi-data.ts` 提取 6 个数据表（UnicodeData/ bidiClass/ mirror/ bracketPairs 等常数表）**

工具脚本：写一次性 Python 脚本 `/tmp/extract_bidi.py`，读 `bidi-data.ts`，输 Rust `pub static BN: &[...] = ...` 常数。

- [ ] **步骤 2：编写对照测试**

`tests/pretext_bidi_data_test.rs`：对随机 Unicode 码点查表，断言与 TS `computeBidiClass(codePoint)` 一致（手算几个对照点：U+0041=L, U+05D0=R, U+0608=AN, U+200F=RLO）。

- [ ] **步骤 3：生成 + 实现**

- [ ] **步骤 4：测试通过 + commit**

```bash
git add src/lyricstyles/sonnet_v2/pretext/bidi_data.rs tests/pretext_bidi_data_test.rs
git commit -m "feat(sonnet_v2/pretext): port Unicode bidi data tables (996 lines TS → statics)"
```

### 任务 2.2:analysis.ts（1458 行字素切分 + 仮名禁則 + 闭合标点）

**文件：**
- 创建：`src/lyricstyles/sonnet_v2/pretext/analysis.rs`

- [ ] **步骤 1：逐行读 analysis.ts 前半（≤730 行）**
- [ ] **步骤 2：逐行读后半**
- [ ] **步骤 3：编写断言测试 `tests/pretext_analysis_test.rs`**
  - CJK 切分：`analyzeText("你好world").chunks` 必须得 `["你好","world"]`（word-like CJK run 分离）
  - 仮名禁則：行末禁 `、` `。`，行首禁 `「` `（`
  - 闭合标点合并 `合う)` 视为一个 run
- [ ] **步骤 4：实现 `analyze_text(text, locale, options) -> TextAnalysis` + export 用 Unicode-segmentation + 手写仮名禁則表**
- [ ] **步骤 5：测试通过 + commit**

### 任务 2.3:line-break.ts（1236 行）

类似结构，逐行读 + 编写 layout_next_line / break_line + 测试 + commit。

### 任务 2.4:measurement.ts（275 行 — FreeType 替代）

**文件：** `src/lyricstyles/sonnet_v2/pretext/measurement.rs`

- [ ] **步骤 1：读 measurement.ts 全文**
- [ ] **步骤 2：编写测试 `tests/pretext_measure_test.rs`** — 对固定 font + size，`measure_text("hello")` 的 advance 与 FreeType 渲染一致（在 Phase 5 写 FreeType 集成后注入；此 Phase 先用 dummy fn 返回 0，仅占位签名）
- [ ] **步骤 3：实现 `get_segment_metrics` 用 unicode-segmentation，加 emoji 矫正 stub 返回 identity**（跳过 DOM 校正，因为 Rust atlas 无 emoji 来源）
- [ ] **步骤 4：commit**

### 任务 2.5:bidi.ts（175 行）+ line_text.ts（107 行）+ rich-inline.ts（518 行）

每个文件一个任务，结构同上：读全文 → 测试 → 实现 → 通过 → commit。

### 任务 2.6:layout.ts（914 行 — pretext 入口，最关键）

**文件：** `src/lyricstyles/sonnet_v2/pretext/layout.rs`

exposes: `prepare`, `prepareWithSegments`, `layout`, `materializeLineRange`, `walkLineRanges`, `measureLineStats`, `measureNaturalWidth`, `layoutNextLine`, `layoutNextLineRange`, `layoutWithLines`, `clearCache`, `setLocale`, plus types `PreparedText`/`PreparedTextWithSegments`/`LayoutCursor`/`LayoutResult`/`LayoutLine` 等。

- [ ] **步骤 1：编写入口测试 `tests/pretext_layout_test.rs`**
  - `measure_text("测试一二三", font, 24)` = folia `layoutWithLines(prepareWithSegments("测试一二三", font), 99999, 24*1.2)[0].width`（同 Foliation Typographymeasure 语义）—— 此测在 Phase 5 FreeType 接入后跑通，此先 skip
  - `layout_with_lines(prepared, maxWidth=100, lineHeight=1.2)` 产 `lines.len() >= 2`
  - CJK 段每字 break（禁則：行末不孤 `，`）
- [ ] **步骤 2：逐行译** layout.ts 706-911 — 主算法 + cache 管理
- [ ] **步骤 3：注入 measurement.rs 的 advance 计算（this phase 内 dummy）**
- [ ] **步骤 4：测试通过 + commit**

---

## Phase 3：纯算法层移植（无 PIXI、无数值状态机依赖的纯函数）

**目标：** 逐行译 folia sonnet 纯算法层——`sonnetRandom`/`sonnetMotion`/`sonnetCameraTracking`/`sonnetTransitions`/`sonnetSemantic`/`sonnetProgram`，加上外部 `graphemeTiming`/`renderHints` 二 util。全部零 PIXI 依赖，按编译器级 literal 译：`Math.imul` → `u32::wrapping_mul`、`>>> 0` → `as u32`、`Math.PI` → `std::f32::consts::PI`。

### 任务 3.1:random.rs（21 行 TS）

**文件：** 创建 `src/lyricstyles/sonnet_v2/random.rs` + `tests/random_test.rs`

- [ ] **步骤 1：编写失败测试** — `hash_sonnet_seed("sonnet") == 2604911188`（手算：FNV-1a 32bit 后 `>>>0`），`mix_sonnet_seed(0,1) == 2654435761`，`sonnet_hash01(0,0,0)` 在 `[0,1)`
- [ ] **步骤 2：运行验证失败**
- [ ] **步骤 3：实现** — `wrapping_mul` 替代 `Math.imul`，`as u32` 替代 `>>>0`，`/ 4294967296.0` 保 float64→f32 截断一致
- [ ] **步骤 4：测试通过**
- [ ] **步骤 5：commit** — `feat(sonnet_v2): port sonnetRandom (FNV-1a 32 + Knuth multiplicative mixing)`

### 任务 3.2:grapheme_timing.rs（154 行）

**文件：** 创建 `grapheme_timing.rs` + `tests/grapheme_timing_test.rs`

`splitLyricGraphemes` → 用 `unicode_segmentation::Graphemes`；`buildLineGraphemeTimeline` 的 word→line 回填算法字面译。

- [ ] **测试**: `split("你好") == ["你","好"]`；`build_line_grapheme_timeline(line{full_text:"ab",words:[{text:"a",start_ms:0,end_ms:500},{text:"b",start_ms:500,end_ms:1000}]})` → grapheme[0].startTime==0, grapheme[1].startTime==0.5, wordIndex 字段对齐。
- [ ] **实现 + 通过 + commit**

### 任务 3.3:render_hints.rs（243 行）

移植 renderHints.ts：MICRO_LINE_DURATION_THRESHOLD=0.10, SHORT_LINE_DURATION_THRESHOLD=0.18, MICRO_LINE_RENDER_FLOOR=0.067 等常数 literal 译。

- [ ] **测试**: `get_line_render_end_time(line{raw_duration:0.05,...})` 返回 `max(end, start+0.067)`；`line.raw_duration==0.2` 时 transition_mode==normal, word_reveal==normal。
- [ ] **实现 + 通过 + commit**

### 任务 3.4:semantic.rs（100 行 — buildSonnetSemanticSegments）

移植 sonnetSemantic.ts。`Intl.Segmenter(granularity:word)` → 用 `unicode_segmentation::UnicodeSegmentation` 的 `split_word_bounds()` + 手写 `isWordLike`（letter/digit+certain mark run，非标点/空格）。`PUNCTUATION_ONLY` 正则 `/^[\s\p{P}\p{S}]+$/u` → `unicode-segmentation` 的 char class 检测 `char.is_whitespace() || is_punct(c) || is_symbol(c)`，但 Rust `char::is_punctuation()`/`is_numeric()` 与 Unicode P/S category 不完全等价，用 `unicode_bidi_class` 或手动 ranges。sticky 算法字面译。

- [ ] **测试**: "你好world" → 2 segments; "hello!" → 1 segment (sticky punctuation merges); "合う)" → 1 segment。
- [ ] **实现 + 通过 + commit**

### 任务 3.5:motion.rs（282 行 — 全部 ease + camera + breath + shake）

移植 sonnetMotion.ts。所有 cubicBezier 参数 `(0.65,0,0.35,1)` `0.13,0.31`/`0.11,0.29` 等 literal 不动。12-iter 二分求解。`resolveShotPathProgress` 的 7 个 `frames` 分支表字面译。

- [ ] **测试**: `ease_sonnet_expo_out(1.0)==1.0`; `ease_sonnet_in_out(0.5)` 介于 0.45-0.55; `resolve_shot_path_progress('tracking-ribbon', 0.5) ≈ 0.5325` (linear*0.55 + ease_in_out(0.5)*0.45); `resolve_sonnet_camera_breath(0, 0)` 含 sin(0)/cos(0)=0, 各分量==0。
- [ ] **实现 + 通过 + commit**

### 任务 3.6:camera.rs（45 行）

移植 sonnetCameraTracking.ts。`trackingFactor=0.5` 默认。线性插值 focus 点。

- [ ] **测试**: 空数组返 (0,0); 单 glyph t<=startTime 返 blend 后 first 位置; t 在两 glyph 中间按 progress*(1-trackingFactor)。
- [ ] **实现 + 通过 + commit**

### 任务 3.7:transitions.rs（152 行）

移植 sonnetTransitions.ts。`fast-blur`/`mono-glitch`/`camera-pull` 三 kind; `alpha/blur/glitch/glitchSeed` 计算; `resolveSonnetShotTransitionFrame` 前后 shot transition 调度。glitchSeed=`seed*0.0001 + step*0.173` literal。

- [ ] **测试**: `idle_transition_frame` 各字段 == default; `resolve_sonnet_transition_effect_frame('fast-blur','exit',0.5,42)` alpha==1-eased, blur==14*eased; glitchSeed `<=0` 时 alpha 仍为 1。
- [ ] **实现 + 通过 + commit**

### 任务 3.8:program.rs（265 行 — compileSonnetProgram + findShot）

这是核心：段落切分→段落类型分类→shot 分配→transitionOut 分配。所有 hardcode：paragraph_gap_threshold `clamp(median*2.5, 1.25, 3.5)`; splitOversizedDraft 纵深循环 (max 6 行, 18 秒) 带 1000 loopGuard; shotGroupCandidates gap max; zoomBase/zoomSpan per kind; chooseWithoutRepeat hash 算法; transitionDuration `min(0.3, max(0.16, gap>0? gap*0.5 : 0.2))`. buildShots 的 `kind:'breath' && shotIndex==0 && wordCount<=2 → 'quiet-tableau'` 撩 `kind:'chorus' && kind==='quiet-tableau' → 'type-impact'` 覆写——literal。

- [ ] **测试**: 用 3 行 `[start=0/3/6s, text:"a/b/c"]` 构造 program，断言 paragraphs.len()==1 (gap=3<3.5 threshold), shots.len()<=1 (group<4), shot.kind 是 7 kind 之一, transitionOut.kind 是 3 kind 之一.
- [ ] **实现**: 注意 `findSonnetParagraphIndexAtTime` 实施"最大 start<=t"的 `.rev()` 行为——关键！否则 S3 bug 重现。find_shot 加 `t<s.end` 守卫。
- [ ] **通过 + commit**

### 任务 3.9:types.rs（95 行 + 拼装）

把 types.ts 所有 type/interface 翻 Rust enum/struct。`SONNET_TRANSITION_KINDS` 异 `as const`-array。

- [ ] **测试**: 编译即可（类型性）。
- [ ] **实现 + 通过 + commit**

---

## Phase 4：排版层（typography roles + layout + glyph layout + shot flow + poster blocks）

**目标：** hero/semi-hero/support/decoration 角色评分、textFlow 7 种 shot layout、glyph layout、poster blocks 全部字面译。

### 任务 4.1:typography_roles.rs（114 行）

移植 sonnetTypographyRoles.ts。`scoreSonnetHeroSegment`（基于 isWordLike + length）、`findSonnetHeroSegmentIndex`（max score）、`findSonnetSemiHeroSegmentIndex/Indices`、`resolveSonnetRoleFontWeight`。

- [ ] **测试**: `score('hello') > score('!')`; `find_hero_index(['hello',''])==0`; `find_semis` 返回 max-second-scoring segment。
- [ ] **实现 + 通过 + commit**

### 任务 4.2:typography_layout.rs（404 行）

移植 sonnetTypographyLayout.ts。`resolveSonnetTypographyLayout` 是 typography 入口；内部用 pretext::layout_with_lines (Phase 2)。`fontSpec` 串重构（font stack + size + weight）。

- [ ] **测试**: 用 stub measure_text 返 identity，断言 layout placements 数量 = segments 数量; layout.scale 各 role 与 hero_scale/support_scale 公式一致。
- [ ] **实现**: 接 Phase 5 的 FreeType measurement; 此 phase 用 stub。
- [ ] **通过 + commit**

### 任务 4.3:glyph_layout.rs（77 行 — buildSonnetGlyphLayout）

buildSonnetGlyphLayout 按段 gcSegment 转单字 glyph placements。

- [ ] **测试**: `build_glyph_layout([segment{text:"你好", ...}])` 返 2 GlyphPlacement。
- [ ] **通过 + commit**

### 任务 4.4:shot_flow_layouts.rs（557 行 — 7 layout 函数）

7 个 layout 函数: quietTableau / trackingRibbon / editorialColumn / fragmentCollage / crossStack / placeWithGlobalFit / resolveSonnetFlowGaps. `resolveSonnetFlowGaps` 返 `{gap, cap}`。所有 hardcode linear interpolation weight「insert:layout*1.4×」等保持。

- [ ] **测试**: 每 kind 包 unit fixture 测，断言 placement 坐标分布。
- [ ] **实现**: 此 phase 拆 3 任务看长度：4.4a quietTableau/trackingRibbon/editorialColumn; 4.4b fragmentCollage/crossStack; 4.4c placeWithGlobalFit/resolveFlowGaps — 各自 commit。
- [ ] **通过 + commit**

### 任务 4.5:poster_blocks.rs（331 行 — layoutSonnetPosterBlocks）

- [ ] **测试**: 4 行 fixture，断言 layout 产 4 块，每块位置在 layout 规则内。
- [ ] **实现 + 通过 + commit**

---

## Phase 5：FreeType 集成 + atlas byte-identity（G1 方案）

**目标：** 替换 `fontdue` SDF 光栅 → FreeType coverage；接入 Phase 2 的 measurement.rs 让 `measure_text` 走 FreeType advance；让 atlas byte-identity 与 Foliation 参考光栅一致。

### 任务 5.1:freeglue.rs FreeType 封装

**文件：** 创建 `src/lyricstyles/sonnet_v2/freeglue.rs` + `tests/freeglue_test.rs`

- [ ] **步骤 1：编写测试** — load font（本地 `.ttf`），render 'A' @ ppem=64，输出的 coverage buffer size == width*height u8，中心 row 各 alpha > 0。
- [ ] **步骤 2：实现** `struct FreeTypeLib{library: freetype::Library}`, `fn render_glyph(font, char, ppem, load_flags) -> Coverage{width,height,buffer:Vec<u8>}`. 用 `FT_Render_Glyph` mode NORMAL。
- [ ] **步骤 3：byte-identity 对照 fixture** — 把 Foliation 端 issuer 测一次同 `char/size/font`，离线 dump coverage 为 `baseline_*.bin`，Rust 端 `coverage == baseline`。
- [ ] **步骤 4：通过 + commit**

### 任务 5.2:atlas 接 FreeType coverage（sdf.rs 兼容）

**文件：** 修改 `src/sdf.rs:104 GlyphAtlas::spawn`, `:264 ensure_text`, 调 freeglue.rs 取代 fontdue 光栅。保留 `atlas_bytes()` 的 `&[u8]` 签名不变，但 tield content 现在为 coverage 不是 SDF。

- [ ] **测试**: `ensure_text("A", 0)` 后 `atlas_bytes()` 至少 one byte>0。
- [ ] **实现 + 通过 + commit**

### 任务 5.3:draw.rs WGSL scene_at 取样段改写

**文件：** 修改 `src/draw.rs:1948-2145` 的 `fn scene_at` glyph 取样段。从 SDF `smoothstep(0.5-d, 0.5+d, d)` 改为 coverage 直取 `alpha = coverage`。blur/halftone/grain 经 coverage 加权。

- [ ] **测试**: snapshot 渲染单 glyph，肉眼比对锐边与 FreeType 一致。
- [ ] **实现 + 通过 + commit**

### 任务 5.4:pretext measurement 接 FreeType advance

**文件：** 修改 `src/lyricstyles/sonnet_v2/pretext/measurement.rs` — Phase 2 stub 替真 FreeType advance (harfbuzz shaping for combining marks/CJK cluster)。

- [ ] **测**: `measure_text("hello", font, 16)` 在 误差 <1.5 px 内与 Foliation `measureText("hello", font, 16)` 一致。
- [ ] **通过 + commit**

---

## Phase 6：arena + scene_builder + text_view_builder（核心 X 架构）

**目标：** 实现 SonnetSceneArena + buildSonnetScene + buildSonnetTextView; PixiJS 的 `new Container/Sprite/Glyph` → arena node insertion; field-init 字面映射 folia SceneView/ShotView/GlyphView。

### 任务 6.1:arena.rs（SonnetSceneArena + ID newtypes + flatten）

**文件：** 创建 `src/lyricstyles/sonnet_v2/arena.rs` + `tests/arena_test.rs`

- [ ] **步骤 1:类型** — `newtype` family（SceneId/ShotId/SegmentId/GlyphId/GhostId/MgLayerId/GuideId/FrameDecorId 都 `struct N(u32);`），`struct GlyphNode{display: MutFields, halo: Option<GlyphId>, ca_cyan: Option<GlyphId>, ca_red: Option<GlyphId>, ghosts: Vec<GhostId>, base_x, base_y, start_time, settle_time, role, ...}`, `struct SceneArena{arena.field_ArenaTables plus runtime_state: SonnetRuntimeState}`. 用 `typed_arena`-style wrappers 是过度——直接用 `Vec<...>` + `Index` impl。
- [ ] **步骤 2:flatten** — `fn flatten_scene(&self, scene_id: SceneId, out: &mut Vec<CharQuad>)` 遍 arena node 产 CharQuad. ordering: descendant DFS, skip alpha<=0.004 and invisible.
- [ ] **测试**: 单 glyph arena node，flatten 产 1 CharQuad，无 halo 时 alpha==display.alpha。
- [ ] **实现 + 通过 + commit**

### 任务 6.2:scene_builder.rs（346 行 — buildSonnetScene）

移植 sonnetSceneBuilder.ts。逐字段建 arena 的 SceneNode + 调子模块建 shot/segment/glyph/mg/guide/frame_decor node。`showOnlyText` 等 tuning choices 在 build 阶段 resolve (而非 render frame)。

- [ ] **测试**: 单段落 fixture，arena 产 1 scene, 1+ shot, 各 shot 含 ≥1 glyph node。
- [ ] **实现 + 通过 + commit**

### 任务 6.3:text_view_builder.rs（420 行 — buildSonnetTextView）

移植 sonnetTextViewBuilder.ts。glyph→arena GlyphNode 构造; caCyan/caRed/ghost sub-glyph 节点。`isTextGlyph` flag 偏 push_word_full 时用。

- [ ] **测试**: 1 glyph segment 产 1 GlyphNode + 若 role==support 0 ghosts; role==hero 2 ghosts。
- [ ] **实现 + 通过 + commit**

### 任务 6.4:guides.rs + frame_decor.rs

- [ ] **guides.ts 270 行**: createSonnetGuide 几何 line; arena GuideNode 缺省落 query name 而非遍。
- [ ] **frame_decor.ts 302 行**: buildSonnetFrameDecor + resolveSonnetFrameDecorSpec.
- [ ] **各 commit**

---

## Phase 7：MG（mechanical-graphics）几何系移植（16 文件 ≈ 5136 行）

**目标：** Foliation Mg 矢量笔法全部 transplant 为 Rust 的 closure/FnMut-path target，产 `CharQuad` via `SLOT_TRI=252.0` (三角形 draw.rs path)。`AnimatedGraphics` → `MgGeoLayer` arena node，累计 `moveTo/lineTo/bezierCurveTo/stroke + fill`。

### 任务 7.1:animations.rs（235 行 — AnimatedGraphics）

`AnimatedGraphics(pixi)` → `struct MgGeoLayer{paths:Vec<PathCmd>}`，paths 用 `enum PathCmd{MoveTo(f32,f32), LineTo(f32,f32), Bezier{c1,c2,to}, Stroke{color,width,alpha}, Fill{color,alpha}}`。

- [ ] **测试**: 三个 cmd 序列化正确，区别 move/line stroke。
- [ ] **实现 + 通过 + commit**

### 任务 7.2:shot_mg.rs 中央 dispatcher（814 行 — sonnetShotMg.ts）

是 Mg 主调度。shot kind → 几何工厂列表。逐 case 字面译。literal 不动：`heroScale*1.4`, `radius*0.04` 等。

- [ ] **测试**: 各 shot kind 入参测试 factory 提交的 path 数量与几何参数在范围内。
- [ ] **实现**: 拆 7.2a/b 两次 commit (前 400 行/后 414 行)。
- [ ] **通过 + commit**

### 任务 7.3:Mg family 逐 kind（10 文件）

sonnetShotMgMarine/Craft/Celestial/Kinetic/Music/Architecture/Landscape/Flora/Botanical/Viewport; 各 kind 一个 子任务，约 50-340 行/文件；所有 `target.moveTo(...).lineTo(...)` 链字面译为闭合 `MgGeoLayer` push。

- [ ] **测试**: 每 kind 单测 path 数 + 关键 cmd literal。
- [ ] **实现 + 通过 + commit 每 kind**

### 任务 7.4:themed/extended/additional Mg primitives (67+37+229 行)

sonnetThemedShotMg + Primitives + ExtendedShotMg + AdditionalShotMg. dispatcher helper + base shapes。

- [ ] 每个 commit**

### 任务 7.5:background + open_frame + fixed_geo variants + spatial + text fixed geo（约 1540 行）

- 7.5a: sonnetBackgroundMgVariants (349) + sonnetBackgroundDecor (272)
- 7.5b: sonnetOpenFrameShotMg (347)
- 7.5c: sonnetFixedGeoVariants (224) + sonnetTextFixedGeo (190)
- 7.5d: sonnetSpatialMgGeometry (148)

各折入 arena Mg layer；每段 unit 测 + commit。

---

## Phase 8：基础设施层（credits + postprocess + filters + staff + debug overlay）

### 任务 8.1:credits.rs（153 行）

buildSonnetCreditsPoster + resolveSonnetCreditsFrame + hasSonnetCreditsMetadata (走 arena 的特殊"poster"scene)。每帧 mutate credits container alpha/scale/offset。

- [ ] **测 + 实现 + 通过 + commit**

### 任务 8.2:post_process.rs（136 行）

每帧 uniform amount/seed 计算。我们把 folia 三个 Filter 价格逻辑映射到 `LyricFx` 字段 + `lyric_fx` u32 buffer 填充。所有 fast-blur = `blur*14`, mono-glitch = `glitchSeed = seed*0.0001+step*0.173` 字面。

- [ ] **测**: transition frame `SonnetSceneTransitionFrame` 输入与 Phase 3.7 transitions rs 同样 + 注入 `set_lyrics_fx` 无错。
- [ ] **实现 + 通过 + commit**

### 任务 8.3:filters.rs (Glitch/Lens/Print 3 个 TS 文件 — GLSL 已被 draw.rs 已有 WGSL，此任务仅迁移 uniform 序列值计算)

- [ ] **测**: 在不修改 draw.rs WGSL (Phase 5.3 已并入 lens/blur/glitch)。仅维护 `LyricFx` 字段装配。
- [ ] **实现 + 通过 + commit**

### 任务 8.4:staff_view.rs + staff_notation.rs (164+41 行)

器乐模式 (no lyrics).buildSonnetStaffView → staff lines + notation. 简单 polyline。

- [ ] **测 + 实现 + 通过 + commit**

### 任务 8.5:debug overlay (复刻 sonnetDebug 191 行)

`PULSE_RING_DEBUG_PREVIEW=1` 时 eprintln per-frame place-holder + active glyph 数量。不画 canvas overlay。

- [ ] **测**: 运行一次 preview, stderr 输 `scene= shot= glyph=N`.
- [ ] **实现 + 通过 + commit**

---

## Phase 9：runtime 主循环 + 切 dispatch + 集成 snapshot 测试 + GUI 自检

### 任务 9.1:runtime.rs + runtime_state.rs (createSonnetPixiRuntime 745 行)

包 `SceneCache:LruMap<usize,SceneId>` + carryover state (outro_blur_scene: Option<SceneId>, prev_active_shot, active_para_idx). `render_frame(t)` 主体:
1. find_para_index(t)
2. ensure_scene(p-1,p,p+1); prune_scenes(p)
3. 取 active scene + shot; apply shot container motion (mutate ShotNode.display.*)
4. 各 glyph 节点 mutate display.alpha / scale / rotation / position via Phase 3 motion
5. transition frame mutate alpha/blur/glitch; set lyric_fx uniform
6. flatten → Vec<CharQuad>

- [ ] **测**: churn test — 同 input 跑 300 帧 t=0..15, flatten 输出 no crash; 至少第一帧 quads>0。
- [ ] **实现**: 拆 9.1a (ensure/prune) / 9.1b (mutate loop) / 9.1c (transition fx) 三 commit。
- [ ] **通过 + commit**

### 任务 9.2:9 snapshot 对照测试

9 个 `snapshot_eq_*.txt` — sonnet_v2 + 旧 sonnet 在 9 个固定场景每帧 flatten 的 quads count + roles 兼容性比二维码。每个 snapshot 一对 fixture 文件。

snapshot 主题：
1. `snapshot_eq_quiet_tableau.txt` - quiet tableau layout 静态对位
2. `snapshot_eq_tracking_ribbon.txt` - tracking ribbon 一帧中段
3. `snapshot_eq_editorial_column.txt` - editorial column 多行散列
4. `snapshot_eq_fragment_collage.txt` - fragment collage 副歌段
5. `snapshot_eq_mask_reveal.txt` - mask reveal 入场瞬间
6. `snapshot_eq_poster_blocks.txt` - poster blocks 多段分立
7. `snapshot_eq_transition_fast_blur.txt` - 每段间 fast-blur 帧已知 quads 数
8. `snapshot_eq_transition_mono_glitch.txt` - mono glitch fx 切换
9. `snapshot_eq_outro_credits.txt` - outro credits 出现/消失 alpha ramp

- 每 snapshot 一个 `tests/sonnet_v2/snapshot_eq_*.rs` 测试文件。
- [ ] 全部测 + commit**

### 任务 9.3:切 dispatch — `src/lyricstyles/mod.rs` 切 `sonnet_v2::build_frame`

**文件:** 修改 `src/lyricstyles/mod.rs:LyricStyle::Sonnet arm`

- [ ] 从 `crate::lyricstyles::sonnet::build_frame` 切到 `crate::lyricstyles::sonnet_v2::build_frame`.
- [ ] `cargo check` + `cargo test --all` 0 error.
- [ ] `git commit -m "feat(sonnet): switch dispatch to sonnet_v2 (compiler-grade 1:1 folia port)"`

### 任务 9.4:GUI 自检 (preview 渲染三种 t)

利用已有 `src/preview.rs` 路径跑 3 个时间点 (1.0/5.0/8.0) 写 PNG, 用 vision skill 肉眼比照 Foliation 渲染同输入的 PNG，输出段比对差异小结。

- [ ] `cargo run --release --bin pulse-ring -- preview ...` (如 preview 命令存在)。
- [ ] 输出写到 `docs/superpowers/plans/2026-08-15-sonnet-1to1-rewrite.md` 末尾 `## 自检结果`。
- [ ] 最后 commit `chore(sonnet): GUI self-check report`。

---

## 自检

照 writing-plans 自检节，以全新视角审视：

**1. 规格覆盖度** — Foliation sonnet 55 文件 vs 本 plan 任务对应：

- 纯算法 (motion/camera/transition/semantic/program/random/types/grapheme-timing/render-hints/typography-roles/typography-layout/glyph-layout/shot-flow-layouts/poster-blocks): Phase 3 (9 任务) + Phase 4 (5 任务) ✅
- pretext 8 文件 (layout/analysis/line-break/measurement/bidi/bidi-data/line-text/rich-inline): Phase 2 (任务 2.1-2.6) ✅
- arena/scene-builder/text-view-builder/guides/frame-decor: Phase 6 ✅
- 16 Mg 文件 + great background/open_frame/fixed_geo/spatial/text-fixed-geo: Phase 7 ✅
- credits/postprocess/filters/staff/debug: Phase 8 ✅
- runtime 745 行: Phase 9 ✅
- React UI 4 (entry/Visualizer/Settings): 等价物为 pulse-ring 既有 QML 设置 + StyleCtx.mg_* 开关 — 接入 Phase 0/9（缺:见下）⚠️ **缺漏补充任务 9.5**
- sonnetIcons SVG: 等价物为 polylines atlas fallback — 缺任务补充 ⚠️ **缺漏补充任务 7.6**
- sonnetPixiResources/TexturePool: arena `unload_scenes` runtime helper — 缺任务 ⚠️ **缺漏补充任务 6.5**
- tuning.ts 10 行: StyleCtx.mg_* 已映射 ✅
- sonnetDebug: Phase 8.5 ✅

**2. 占位符扫描** — Phase 描述里，"每 kind 单测"、"每段 commit"、"测 + 实现 + 通过" 这些是骨架句；具体每任务实际执行时仍要补 `pub fn xxx` 的 row code 课例。

PLAN 显示三个 待补缺漏（已在自检发现并补进 Phase 9 末尾）：

### 任务 6.5 (补漏)：arena `unload_scenes ±1` helper + resource 托管
等价 sonnetPixiResources.ts/TexturePool.ts — Rust 端 atlas 是单一 GlyphAtlas，但 scene 词表索引的"释放"由 `SonnetSceneArena::prune_scenes(p)` 调用，此任务集中测 `prune_scenes` |\||≤1 的边界 \ 保证 sceneCache 容量

### 任务 7.6 (补漏)：polyline icon fallback atlas (替代 sonnetIcons SVG)
七个 icon 形状（音符/花/雪花/棱镜/五线谱）用 `push_word_full` 产 polyline quads (rotate overlap = `SLOT_TRI=252.0`)；在 atlas 时由 `mg_decor` flag 决定启用。测: `showIcon=true` 时预览有 ≥1 形状 quad in in mg layer.

### 任务 9.5 (补漏)：QML 设置项映射 tuning
4 React 文件等价物分两步：(1) `StyleCtx` 增加布尔/枚举 `show_guide / show_fixed_geo / typography_motion / camera_intensity / lyrics_font_scale / show_background_decor / show_giant_decorative_text / enable_transitions / outer_frame_mode` 等字段；(2) QML/CLI 开关映射。Cargo check zero error.

**3. 类型一致性** — Phase 1 `LyricWord::syllables` 在 Phase 3.2 grapheme_timing 实现被 `word.syllables` 消费；Phase 3 的 `SceneArena` 在 Phase 6 重构本文定义，Phase 9.1 使用。`SonnetTransitionFrame` 用 Phase 3.7 定义名，Phase 8.2 填充触发一致（如改 named fork 立即自检再改正）。

完成。下一步交执行：**主会话亲自**按 Phase 0→9 顺序逐任务实现（不 fan-out，access tier 限时限 confirmed），每任务 TDD + cargo check + commit，最后 GUI 自检。
