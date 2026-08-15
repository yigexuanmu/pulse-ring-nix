# 解耦计划：歌词层独立于壁纸子系统

> 起因：移植 folia 时把歌词层寄生在了壁纸子系统上（住 `assets/wallpapers/`、靠
> `scene_wallpaper` 字段启用、复用 `scene_player`、帧路由抢进 `folia_overlay_image`、
> 轮播还会清掉歌词）。壁纸是副产品，歌词是第一公民功能，必须彻底解耦。

---

## 一、当前耦合点（实测，附行号）

| 位置 | 现状（错误） | 性质 |
|------|-------|------|
| `assets/wallpapers/folia-lyrics/` | 歌词作为"壁纸包"放在壁纸库目录 | 资源放错地方 |
| `config.rs:337` `scene_wallpaper` | 唯一启用 folia 的开关 | 壁纸字段被滥用 |
| `main.rs:408` `scene_player` | 双重身份：真场景壁纸 + 歌词页 | player 复用 |
| `main.rs:2380` `web_player` 帧路由 | web 壁纸帧 → `folia_overlay_image`（中层）而非 `wallpaper_image`（底层）| **真 bug：web 壁纸当不了底层** |
| `main.rs:2364` `scene_player` 帧路由 | scene 帧也 → `folia_overlay_image` | **真 bug：scene 壁纸当不了底层** |
| `main.rs:2192` `push_folia_state` | 歌词/playback/theme 推给 `web_player`+`scene_player` | 把歌词塞进真壁纸 |
| `main.rs:2445` `tick_wallpaper_rotation` | 轮播切壁纸时 `folia_overlay_image=None`+`clear_overlay()` | **真 bug：切壁纸清掉歌词** |

---

## 二、目标架构（三层 z-order 不变）

```
Pass 1   底层壁纸  ← image / video / web / scene / 轮播列表   槽: wallpaper_image
Pass 1.5 中层歌词  ← folia_player(独立)                      槽: folia_overlay_image
Pass 2   环/粒子   ← 原生 wgpu 渲染                           （不变）
```

壁纸任何功能（image/video/web/scene/轮播）原封不动走 `wallpaper_image`；
歌词走独立的 `folia_player` → `folia_overlay_image`，永不碰壁纸字段、壁纸库、壁纸槽。

---

## 三、改动清单（按文件，标注改/新增/删）

### 1. `config.rs`（改）
- 新增字段 `folia_lyrics: Option<String>`（值=可视化模式名 classic/cadenza/.../sonnet；`None`=关闭）。
- `Config::default()`：`folia_lyrics: None`（不影响现有用户）。
- QML 解析（对齐 `scene_wallpaper` 的写法 ~L993）：新增
  `"foliaLyrics" => cfg.folia_lyrics = Some(s.clone())`。
- `scene_wallpaper` 字段**保留不动**——还给真场景壁纸。

### 2. `src/wallpaper_pack.rs`（新增 helper，不改现有）
- 新增 `pub fn folia_html_path() -> Option<String>`：
  - 优先读环境变量 `PULSE_RING_FOLIA_HTML`（Nix wrapProgram 注入）。
  - 回退 `CARGO_MANIFEST_DIR/folia-wallpaper/dist/index.html`（dev `cargo run`）。
  - **不调 `resolve_wallpaper` / 不查 `library_dir`**——歌词不是壁纸包。

### 3. `src/main.rs`（核心改动）

#### 3a. App 字段（~L241, ~L175）
- 新增 `folia_player: Option<web_wallpaper::WebWallpaperPlayer>`。
- 新增 `folia_first_frame: bool`。
- `scene_player` / `scene_first_frame`：**保留**（变回纯场景壁纸，帧路由改回底层）。
- `folia_overlay_image` / `folia_overlay_dirty`：**保留**（中层，只由 `folia_player` 喂）。

#### 3b. spawn 决策（~L408 起那段）
- 保留 `scene_player` 的 spawn 逻辑（真场景壁纸）。
- 在其后新增独立段：
  ```rust
  let mut folia_player = None;
  if let Some(mode) = &cfg.folia_lyrics {
      if let Some(html) = wallpaper_pack::folia_html_path() {
          let (w, h) = cfg.web_wallpaper_size;          // 复用尺寸配置
          match web_wallpaper::start_web_wallpaper(&html, w, h) {
              Ok(mut p) => {
                  p.send_config(&format!("{{\"visualizerMode\":\"{mode}\"}}"));
                  folia_player = Some(p);
              }
              Err(e) => log::warn!("folia lyrics failed ({e})"),
          }
      }
  }
  ```
- App 构造初始化 `folia_player` / `folia_first_frame: false`。

#### 3c. tick() 帧路由（~L2364 / ~L2380）
- `scene_player` 帧：改成路由到 **`wallpaper_image`**（底层），非 `folia_overlay_image`。
  Scene 真壁纸归底层。
- `web_player` 帧：改成路由到 **`wallpaper_image`**（底层），非 `folia_overlay_image`。
  web 壁纸归底层。
- 新增 `folia_player` 帧路由 → `folia_overlay_image`（中层）：唯一喂中层者。

#### 3d. push_folia_state（~L2192）
- `has_player` 判定改为只看 `self.folia_player.is_some()`。
- 所有 `send_theme`/`send_lyrics`/`send_playback` 调用：
  删掉对 `web_player` / `scene_player` 的推送，**只推给 `folia_player`**。

#### 3e. tick() 音频推送（~L2325 那段）
- `send_audio`：同样**只推给 `folia_player`**。
  （真壁纸页面不需要音频 band——上游 web/scene 壁纸原本也不依赖，去掉无害。）

#### 3f. tick_wallpaper_rotation（~L2445）**最关键的 bug 修复**
- 删除 `self.folia_overlay_image = None; self.folia_overlay_dirty = false; clear_overlay();`。
  切壁纸轮播不该动歌词层。改为：轮播只管 `wallpaper_image`/底层纹理（本就如此）。
- 顶部守卫：`if self.scene_player.is_some() || self.wallpaper_list.is_empty() { return }`
  改为只 `if self.wallpaper_list.is_empty() { return }`（scene 不再是"霸占底层"的角色，
   scene 是底层之一，与轮播列表互斥逻辑由现有 spawn 决策保证）。**评估后若 scene
   与轮播本就互斥，则保留守卫更安全——以实测为准。**

#### 3g. render_output 上传（~L2660）
- `wallpaper_image` 上传判定 `(... || scene_player.is_some())`：保留（scene 帧已在 3c
   路由进 `wallpaper_image`）。
- `folia_overlay_dirty` → `upload_overlay`：保留，只由 `folia_player` 触发。
- `target`（L2402）渲染全部显示器判定：`wallpaper_image.is_some() ||
   folia_overlay_image.is_some()`——保留（歌词铺满多屏本就是预期）。

#### 3h. 拆除（Drop / 无关主权）
- `folia_player` 用完后随 `App` drop 自动 kill（`WebWallpaperPlayer::Drop` 已处理）。
- 无需额外 teardown。

### 4. `flake.nix`（改 postInstall）
- folia 产物安装路径：`cp -r folia-wallpaper $out/share/pulse-ring/folia/`
  （去掉原来混进 assets 的对 folia 的依赖表述）。
- 新增 `wrapProgram --set PULSE_RING_FOLIA_HTML "$out/share/pulse-ring/folia/dist/index.html"`。
- devShell shellHook 加 `export PULSE_RING_FOLIA_HTML="$PWD/folia-wallpaper/dist/index.html"`。

### 5. 仓库资源删除
- 删 `assets/wallpapers/folia-lyrics/` 整个目录（歌词不是壁纸包）。
- 上游 `assets/wallpapers/audio-scene/` 等真场景壁纸包保留不动。

### 6. `config/pulse-ring.qml`（默认模板，改）
- 壁纸注释段保留。
- 新增独立段（不混进壁纸段）：
  ```qml
  // ================= 歌词可视化（folia，独立于壁纸）=================
  // 启用歌词层（中层渲染，在壁纸之上、环之下）。值=可视化模式名：
  //   classic | cadenza | partita | fume | claddagh | cappella | tilt
  //   | monet | diorama | pendolo | sonnet
  // foliaLyrics: "sonnet"
  ```

### 7. `docs/folia-lyrics.md`（改用户文档）
- 把"改 `scene_wallpaper`"全部改成"改 `foliaLyrics`"。
- 把"复制到 `~/.config/pulse-ring/wallpapers/`"删掉（开箱即用，Nix 已装到 share/）。
- 切换 mode 的三种方式里去掉 project.json 那条（已无 project.json），
  改为 `foliaLyrics: "模式名"` + URL `?mode=` 两种。

### 8. 不动的部分
- `src/draw.rs`：Pass 1 / 1.5 / 2 结构不变。中层 pass 已靠 `overlay_texture.is_some()`
  + `upload_overlay` 驱动，数据管线决定图层，渲染器无需感知来源。
- `src/folia_bridge.rs`：签名不变（收 `Option<&mut WebWallpaperPlayer>`）。
- `src/web_wallpaper.rs`：`start_web_wallpaper` / `WebWallpaperPlayer` 通用机制不变
  （歌词只是它的另一个调用者）。
- `folia-wallpaper/` React 工程、bridge 文件、Electron main/preload：全部不动。

---

## 四、执行顺序与验证

1. **config.rs** + **wallpaper_pack.rs**：加字段 + path helper → `cargo check`
2. **main.rs** 3a–3h：一次性解耦 → `cargo check` + `cargo test folia_bridge`
3. **flake.nix** postInstall + shellHook → `nix build`（must pass, 检验 `PULSE_RING_FOLIA_HTML` 注入）
4. 删 `assets/wallpapers/folia-lyrics/`
5. qml 模板 + 用户文档
6. 本地 `nix run` + `RUST_LOG=info` 实测：
   - 不设 `foliaLyrics` → 行为同上游（壁纸功能完好，无 lyrics）
   - `foliaLyrics: "sonnet"` + 放音乐 → 中层出现歌词
   - 同时设 `webWallpaper`+`foliaLyrics` → 底层是 web 壁纸、中层是歌词，互不干扰
   - 轮播切壁纸 → 歌词不被清

---

## 五、风险与回退

- render_output 里 `wallpaper_image` 的 fast-path 判定含 `scene_player.is_some()`：
  scene 帧路由改底层后该判定天然正确，但需实测确认 scene 不再"霸占边角"。
- `tick_wallpaper_rotation` 守卫改动需以实测验证不破坏轮播与 scene 互斥。
- 全程在 `neo` 分支；回退靠 `git revert` 这批 commit。
