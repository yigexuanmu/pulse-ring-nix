# EDT Background-Thread Design (D.3 fix scaffold)

## 现状（PERF_AUDIT §2.3 + RENDER_PIPELINE_AUDIT §C.5/B.3 实测）
- `sdf.rs:23 MAX_NEW_GLYPHS=4`：每 CJK glyph EDT 5-30ms × 4 ≈ 12-120ms/frame 卡顿
- `sdf.rs:158 reset_budget` 每帧消耗：超 cap 即 DEFER 半秒（:167）→ 首歌前 25s 缺字
- `sdf.rs:310 packed count`：CDN burst（CJK 长句）会持续 DEFER 缺尾

## 设计
方案：独立 background thread 持续 fill glyph → SDF atlas，绕过 4-pack/frame cap。
- 主线程只 push pending glyph 列表入无锁 SPSC 队列（crossbeam-queue）
- 后台 thread 消费，执行 128×128 EDT 然后 texture_write_buffer 上传到下一未用 cell
- atlas 共享：Arc<Mutex<GlyphAtlas>> 或双缓冲（writer 写 back buffer，drain 时 swap front/back）

## 同步点所需
- pending glyph queue 无锁 SPSC：producer=主线程 consumer=后台
- atlas Mutex 仅在 drain/swap front/back 时持锁 <1ms
- write queue（GPU submission）必须在主线程 → 后台 thread 用 channel 把 cell_idx+bytes 回传主线程
- 渲染线程在帧头 drain 一次回传 channel

## 风险
1. wgpu Buffer 不可跨线程 → 后台 thread 不能直接 queue.write_texture，需 channel 回传
2. Mutex<GlyphAtlas> 在主线程 hot path 持锁会 statter → 用 double-buffer肚
3. DEFER 半秒延迟语义保留（即使 bg 加速仍可能有 short miss）→ 退化为 "wait for bg done" 状态而不是 DEFER
4. 测试需 sample CJK 长曲前 25s 缺字回归

## 实施分阶段
- Phase A (此 scaffold)：抽 EdtState struct + design doc + 不加线程
- Phase B (后续 PR)：加 SPSC queue + 后台 thread + double buffer
- Phase C：DEFER 退化为 wait-for-bg-done
