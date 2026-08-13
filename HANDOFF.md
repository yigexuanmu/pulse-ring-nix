# Handoff: Pulse-Ring-Nix 4-Pipeline Migration

## 你的任务

你是一个接手此项目的 AI。你的任务是对整个项目的渲染管线及所有相关代码进行
**激进的深度优化**，同时遵守以下硬性要求：

1. **绝不牺牲视觉效果或质量** — 所有现有功能（ring、particles、widgets、lyrics、
   decor quads、glyph SDF、lens、RGB shift、post FX、capture 等）必须保持完全一致的
  渲染输出。
2. **保证一切正常运行** — 完成后 `cargo check` 零错误、`cargo test` 全部通过、
   应用能正常启动和渲染。不遗漏任何一处编译错误，不跳过任何未完成的迁移步骤。
3. **绝不省略任何一处优化** — 不只是让代码编译通过，而是要穷尽所有可行的优化手段：
   GPU shader 性能、CPU 端 buffer 写入、内存布局、pipeline 状态复用、离屏纹理复用、
   uniform 拆分、storage buffer 分离、render pass 合并/剔除、draw call 精简等。
   每一个能优化的点都必须被优化。
4. **先修复、再优化、后验证** — 先完成 4-pipeline 迁移让代码编译通过，再逐层施加
   更深层的优化，最后用 `cargo check` + `cargo test` 验证完整性。

下方文档详述了当前代码状态、架构、以及逐步的修复与优化计划。

## Project Overview

Pulse-Ring-Nix is a Wayland wallpaper-layer music visualization (GPU rendered with wgpu).
The renderer draws a pulsing ring, particles, widgets, and animated lyrics onto a
transparent surface. The project is built with Nix (`nix develop` for dependencies,
`cargo` for compilation).

- **Language:** Rust (edition 2024), WGSL shaders (embedded as `stringify!` constants)
- **GPU API:** wgpu 30
- **Build:** `nix develop /tmp/opencode/pulse-ring-nix#default` then `cargo check` / `cargo build`
- **Surface:** Bgra8Unorm/Bgra8UnormSrgb, PreMultiplied alpha, Mailbox present mode
- **Total source:** ~12,200 lines across 16 Rust files

## Current State: BROKEN (does not compile)

The code is in a **half-migrated state**. The struct `RingRenderer` was edited to declare
a new 4-pipeline architecture, but the `new()` constructor and `render()` method still use
the old single-pipeline architecture. The migration was started but never finished.

### Compilation errors (27 errors total)

Run `nix develop /tmp/opencode/pulse-ring-nix#default --command bash -c "cd /tmp/opencode/pulse-ring-nix && cargo check"` to reproduce.

All errors are in **`src/draw.rs`**:

| Error | Location | Cause |
|-------|----------|-------|
| E0124: `decor_words_data` already declared | line 78 | Duplicate field (also at line 69) |
| E0124: `decor_word_count` already declared | line 79 | Duplicate field (also at line 70) |
| E0124: `glyph_words_data` already declared | line 82 | Duplicate field (also at line 73) |
| E0124: `glyph_word_count` already declared | line 83 | Duplicate field (also at line 74) |
| E0560: no field `pipeline` | line 370 | `new()` still writes old struct field |
| E0560: no field `bind_group` | line 372 | `new()` still writes old struct field |
| E0560: no field `width` | line 373 | `new()` still writes old struct field |
| E0560: no field `height` | line 374 | `new()` still writes old struct field |
| E0560: no field `bind_group_layout` | line 405 | `new()` still writes old struct field |
| E0609: no field `bind_group` | line 520 | `refresh_texture_bindings()` uses old field |
| E0609: no field `bind_group_layout` | line 522 | `refresh_texture_bindings()` uses old field |
| E0609: no field `width`/`height` | lines 636, 639-642 | `resize()` uses old fields |
| E0609: no field `width`/`height` | lines 711, 730 | `render()` uses old fields |
| E0609: no field `pipeline` | line 837 | `render()` uses old field |
| E0609: no field `bind_group` | line 838 | `render()` uses old field |
| E0609: no field `width`/`height` | lines 857-858, 865-866 | `render()` stats + `capture_frame()` |

## Architecture: Old vs New

### Old (current `new()` + `render()` code)
- Single pipeline (`pipeline`) with single bind group (`bind_group`)
- Full-screen triangle `vs_main` + `fs_main` → `scene_at()` does everything
- All lyric/decor/glyph quads packed into `Uniforms` struct as `decor_words`/`glyph_words` arrays
- Single uniform buffer uploaded each frame (~400KB)
- Decor and glyph loops run **per-pixel** inside the main fragment shader

### New (intended target — struct fields + new shaders already declared)
**4 pipelines**, each with its own bind group layout and off-screen render target:

1. **`bg_pipeline`** — Full-screen triangle fragment shader. Renders ring, widgets, particles,
   lens distortion, RGB shift. Writes to `bg_tex` (off-screen `rgba16f`). The per-pixel
   decor/glyph loops should be **removed** from `scene_at()` since they now have their own
   pipelines. The bg shader should only output the ring background.

2. **`decor_pipeline`** — Instanced triangles (one per MG decoration quad, slot >= 252).
   Shader `DECOR_SHADER_SRC` is **already written** (lines 1957-2072). Reads
   `decor_words: array<f32, 30000>` from a storage buffer. Evaluates SDF (triangle/pill/rect)
   per-instance. Writes to `decor_tex` (off-screen `rgba16f`).

3. **`glyph_pipeline`** — Instanced triangles (one per SDF glyph quad, slot < 2).
   Shader `GLYPH_SHADER_SRC` is **already written** (lines 2077-2221). Reads
   `glyph_words: array<f32, 60000>` from a storage buffer. Samples the SDF atlas texture
   with blur/CA/glow/contrast. Writes to `glyph_tex` (off-screen `rgba16f`).

4. **`composite_pipeline`** — Full-screen triangle that samples `bg_tex` + `decor_tex` +
   `glyph_tex` and composites them together (alpha-over), then applies post FX (vignette,
   halftone). **This shader does not exist yet** — `COMPOSITE_SHADER_SRC` must be written.

### Off-screen targets
- `bg_tex`, `decor_tex`, `glyph_tex`: all `rgba16f`, sized to surface × `render_scale`
- Recreated on `resize()` when dimensions change
- `composite_sampler`: linear-filtering sampler for reading back off-screen targets

### Data flow (per frame)
1. CPU writes `Uniforms` to `uniform_buffer` (ring config, bands, particles, widgets, etc.)
   — but **without** `decor_words`/`glyph_words`/`lyric_*` fields (those move to storage buffers)
2. CPU writes decor quads to `decor_instance_buffer` (storage buffer, 30000 f32)
3. CPU writes glyph quads to `glyph_instance_buffer` (storage buffer, 60000 f32)
4. **Render pass 1** (bg): clear `bg_tex` → set `bg_pipeline` → draw(0..3, 0..1)
5. **Render pass 2** (decor): clear `decor_tex` → set `decor_pipeline` → draw(0..3, 0..decor_count)
6. **Render pass 3** (glyph): clear `glyph_tex` → set `glyph_pipeline` → draw(0..3, 0..glyph_count)
7. **Render pass 4** (composite): clear surface view → set `composite_pipeline` → sample bg+decor+glyph → draw(0..3, 0..1)

## What Needs To Be Done

### Step 1: Fix the struct (lines 9-92)
- **Remove duplicate fields**: lines 75-83 are duplicates of lines 66-74. Delete lines 75-83.
- **Add `width: u32` and `height: u32`** fields (they were removed but are still needed).
- Remove `lyric_words_data: [f32; 65536]` and `lyric_word_count: u32` — these are stale
  leftovers from the old single-buffer approach. `set_lyrics()` should only fill
  `decor_words_data` and `glyph_words_data`.
- Consider splitting the `Uniforms` struct: ring config stays in uniform buffer, but
  decor/glyph data moves to storage buffers (the new shaders already expect storage buffers).

### Step 2: Rewrite `new()` (lines 172-407)
Replace the single-pipeline creation with 4-pipeline creation:

1. Create the uniform buffer (without decor/glyph arrays — much smaller now).
2. Create `decor_instance_buffer` and `glyph_instance_buffer` as storage buffers.
3. Create the band texture (1D, pre-smoothed).
4. Create 4 bind group layouts:
   - `bg_bgl`: uniform buffer + widget atlas texture + sampler + lyric SDF texture
   - `decor_bgl`: uniform buffer (small: resolution + decor_count) + decor_words storage buffer
   - `glyph_bgl`: uniform buffer (small: resolution + lyric_time + glyph_count + lyric_fx) + glyph_words storage buffer + lyric SDF texture + sampler
   - `composite_bgl`: bg_tex view + decor_tex view + glyph_tex view + composite_sampler
5. Create 4 bind groups from the layouts.
6. Create 4 render pipelines from the 3 existing shader constants + the new composite shader.
7. Create the `composite_sampler` (linear filtering).

### Step 3: Write `COMPOSITE_SHADER_SRC`
A new WGSL shader that:
- Draws a full-screen triangle
- Samples `bg_tex`, `decor_tex`, `glyph_tex` (all `rgba16f`)
- Composites: bg → decor → glyph (alpha-over, premultiplied)
- Applies post FX (vignette, halftone) currently in `scene_at()` lines 1904-1933
- Outputs to the surface (Bgra8Unorm, premultiplied blend)

### Step 4: Rewrite `render()` (lines 647-861)
Replace the single render pass with 4 passes:
1. Write uniform buffer + decor instance buffer + glyph instance buffer
2. Create `bg_tex`/`decor_tex`/`glyph_tex` views if not yet created (lazily, on resize)
3. Render pass 1 (bg → `bg_tex`): `pass.set_pipeline(&self.bg_pipeline); pass.set_bind_group(0, &self.bg_bind_group, &[]); pass.draw(0..3, 0..1);`
4. Render pass 2 (decor → `decor_tex`): `pass.set_pipeline(&self.decor_pipeline); pass.set_bind_group(0, &self.decor_bind_group, &[]); pass.draw(0..3, 0..self.decor_word_count);`
5. Render pass 3 (glyph → `glyph_tex`): `pass.set_pipeline(&self.glyph_pipeline); pass.set_bind_group(0, &self.glyph_bind_group, &[]); pass.draw(0..3, 0..self.glyph_word_count);`
6. Render pass 4 (composite → surface view): `pass.set_pipeline(&self.composite_pipeline); pass.set_bind_group(0, &self.composite_bind_group, &[]); pass.draw(0..3, 0..1);`
7. Submit encoder, present frame.

### Step 5: Rewrite `resize()` (lines 635-645)
- Store `width`/`height` (re-add fields to struct)
- Recreate off-screen textures (`bg_tex`, `decor_tex`, `glyph_tex`) at new dimensions × render_scale
- Recreate composite bind group with new texture views
- Reconfigure surface

### Step 6: Rewrite `refresh_texture_bindings()` (lines 517-531)
- Update `bg_bind_group` with new atlas/lyric texture views
- (decor/glyph bind groups don't change unless instance buffer size changes)

### Step 7: Update `set_lyrics()` (lines 469-515)
- Remove `self.lyric_words_data` writes (field to be removed)
- Keep only the decor/glyph split logic (already present at lines 495-505)
- After writing CPU-side arrays, also `write_buffer` to `decor_instance_buffer` and `glyph_instance_buffer`

### Step 8: Trim `SHADER_SRC` (lines 938-1951)
- Remove the decor loop (lines 1738-1792) from `scene_at()` — now handled by `decor_pipeline`
- Remove the glyph loop (lines 1811-1893) from `scene_at()` — now handled by `glyph_pipeline`
- Remove the lyric AABB fast-reject logic (lines 1731-1734) — no longer needed in bg shader
- Remove the lens distortion (lines 1726-1730) — move to glyph shader or composite shader
- Remove `decor_words`, `glyph_words`, `lyric_*` fields from the WGSL `Uniforms` struct
  (lines 994-1001) — these move to storage buffers / composite shader
- `scene_at()` should return only ring_rgb + ring_alpha (no lyric compositing)
- The composite shader takes over the final compositing + post FX

### Step 9: Update `capture_frame()` (lines 864-927)
- Replace `self.width`/`self.height` with the new field names (once fields are re-added)

### Step 10: Verify
```bash
nix develop /tmp/opencode/pulse-ring-nix#default --command bash -c "cd /tmp/opencode/pulse-ring-nix && cargo check"
nix develop /tmp/opencode/pulse-ring-nix#default --command bash -c "cd /tmp/opencode/pulse-ring-nix && cargo test"
```

## File Map

| File | Lines | Role |
|------|-------|------|
| `src/draw.rs` | 2235 | **Main renderer** — struct, `new()`, `render()`, 3 WGSL shaders, capture. THIS IS THE FILE TO EDIT. |
| `src/config.rs` | 1271 | Config parser, `LyricStyle`, `WidgetType`, `Config` struct |
| `src/lyricview.rs` | 655 | `CharQuad`, `StyleCtx`, `build_frame` — produces the word quad arrays |
| `src/sdf.rs` | 416 | SDF glyph atlas (fontdue), `GlyphAtlas`, `GlyphInfo`, `PlacedChar` |
| `src/lyrics.rs` | 257 | Lyric fetching (Python CLI subprocess), `LyricLine`, `LyricData` |
| `src/main.rs` | 1609 | Event loop, particle animation, widget layout |
| `src/audio.rs` | 300 | Audio analysis, `NBANDS = 128` |
| `src/lyricstyles/sonnet.rs` | 2028 | Sonnet lyric style (cinematic paragraph/shot animation) |
| `src/preview.rs` | ~100 | Preview rendering helper |
| `src/plugin.rs` | ~200 | Plugin/render request dispatch |
| `flake.nix` | 99 | Nix build + dev shell (alsa, wayland, xkbcommon) |

## Key Data Structures

### `CharQuad` (lyricview.rs) — 20 f32 per quad
| Offset | Field | Meaning |
|--------|-------|---------|
| 0 | slot | 0..1 = SDF glyph, 252 = triangle, 254 = rect/line, 255 = pill |
| 1-2 | uv_x, uv_y | Atlas UV top-left |
| 3-4 | uv_w, uv_h | Atlas UV size |
| 5-6 | w, h | Quad size in pixels |
| 7-8 | x, y | Screen position (center) |
| 9 | scale | Render scale |
| 10 | alpha | Opacity |
| 11 | rotate | Rotation (radians) |
| 12-15 | r, g, b, a | Tint color |
| 16-17 | ext0, ext1 | Extra (v0: CA amount, etc.) |
| 18-19 | v1x, v1y | Extra (triangle vertex for slot=252) |

### `Uniforms` struct (draw.rs lines 98-169) — currently ~400KB
The struct is huge because `decor_words: [f32; 30000]` and `glyph_words: [f32; 60000]` are
packed inside it. In the new architecture these move to separate storage buffers, shrinking
the uniform to ~5KB.

## Shaders Summary

| Shader | Lines | Entry Points | Status |
|--------|-------|-------------|--------|
| `SHADER_SRC` | 938-1951 | `vs_main`, `fs_main` → `scene_at()` | Needs trimming (remove decor/glyph loops + lyric fields) |
| `DECOR_SHADER_SRC` | 1957-2072 | `vs_main`, `fs_main` | **Done** — instanced, reads storage buffer |
| `GLYPH_SHADER_SRC` | 2077-2221 | `vs_main`, `fs_main` | **Done** — instanced, samples SDF atlas |
| `COMPOSITE_SHADER_SRC` | — | — | **NOT WRITTEN** — must create |

## Performance Rationale

The 4-pipeline split exists because the old single-pipeline approach ran two per-pixel
loops (decor ~150 quads + glyph ~30 quads) for **every pixel on screen**, even pixels far
from any lyric. The new approach:

- **bg_pipeline**: one full-screen pass, no loops — only ring/particles/widgets math
- **decor_pipeline**: instanced — GPU only shades pixels covered by each decor quad's bounding box
- **glyph_pipeline**: instanced — GPU only shades pixels covered by each glyph quad's bounding box
- **composite_pipeline**: one full-screen pass, 3 texture samples + post FX

This reduces fragment shader work from O(pixels × quads) to O(pixels + covered_pixels × quads).

## Build & Test Commands

```bash
# Enter dev shell (provides alsa, wayland, xkbcommon, pkg-config, rustc, cargo)
nix develop /tmp/opencode/pulse-ring-nix#default

# Type check
cargo check

# Build
cargo build --release

# Run tests (includes WGSL validation test)
cargo test

# Run the app
cargo run --release
```

## Notes

- The `stringify!` macro is used for WGSL shaders, so they are embedded as string literals.
  This means no runtime file I/O for shaders, but also means syntax errors only show at
  runtime (the `#[test] shader_is_valid_wgsl` test catches them in CI).
- The surface alpha mode is `PreMultiplied` — all blend states use `SrcFactor::One` /
  `DstFactor::OneMinusSrcAlpha` (premultiplied alpha-over).
- `render_scale` (0.25..1.0) controls off-screen render resolution. Lower = fewer pixels
  in all 4 passes, compositor upscales the final surface.
- The app supports multiple monitors (each `RingRenderer` has an `id`).
- The `lyric_texture` is an R8Unorm 4096×4096 SDF atlas (single channel, fontdue-rasterized).
- The `atlas_texture` is an Rgba8UnormSrgb 2048×2048 widget image atlas.
