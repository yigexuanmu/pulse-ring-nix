//! Foliate歌词可视化配置 (GUI ↔ Rust ↔ Electron ↔ folia 页) 的共享层。
//!
//! 配置文件：`$XDG_CONFIG_HOME/pulse-ring/folia-lyrics.json`（缺省 `~/.config/pulse-ring/folia-lyrics.json`）。
//!
//! 结构：
//! ```jsonc
//! {
//!   "activePreset": "默认",
//!   "presets": {
//!     "默认": {
//!       "enabled": true,                 // false → 即使 scene_wallpaper 已设也不加载 folia
//!       "visualizerMode": "sonnet",       // 11 模式之一
//!       "foliaTuning": { ... }            // Partial<VisualizerTuningBundle>，见 default_bundle()
//!     }
//!   }
//! }
//! ```
//!
//! 两个消费者：
//!   - `pulse-ring` 主二进制：`merge_config_payload()` 把激活预设的 `visualizerMode`
//!     与 `foliaTuning` 合并进 wallpaper pack 的 params，经 `send_config` 推给 Electron。
//!   - `pulse-ring-config` GUI：`load()`/`save()` 管理预设（新建/切换/重置/分组保存）。
//!
//! 全部 11 个模式的默认 Tuning 都从 folia 的 `DEFAULT_*_TUNING`（types.ts）逐字抄写，
//! 以保证"重置" 回到 folia 设计默认；`default_bundle()` 给出完整 bundle 供首次自动生成。

use serde_json::{json, Value};

/// 11 个 folia 歌词可视化模式名（与 folia VisualizerMode / VisualizerTuningMode 对齐）。
pub const MODES: [&str; 11] = [
    "classic", "cadenza", "partita", "fume", "claddagh", "cappella", "tilt",
    "diorama", "pendolo", "monet", "sonnet",
];

/// 缺省预设名（由 GUI 与 Rust 共认，初始化时用它）。
pub const DEFAULT_PRESET: &str = "默认";

/// 配置文件绝对路径（沿用 pulse-ring 的 XDG 约定，与 qml 同目录）。
pub fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(&home).join(".config"));
    base.join("pulse-ring").join("folia-lyrics.json")
}

/// 全 11 模式默认 Tuning Bundle（逐字抄自 folia types.ts 的 DEFAULT_*_TUNING）。
/// 任何 GUI 不可编辑、或用户未改的字段都以此为准。
pub fn default_bundle() -> Value {
    json!({
        "classic": {
            "enableWordRotation": true,
            "breathingFloatMultiplier": 1.0,
            "useLegacyLayout": false,
            "wordSpacing": 0.7
        },
        "cadenza": {
            "fontScale": 1.12,
            "widthRatio": 0.72,
            "motionAmount": 1.0,
            "glowIntensity": 1.0,
            "beamIntensity": 0.0
        },
        "partita": {
            "showGuideLines": true,
            "useSemanticLayout": true,
            "staggerMin": 20,
            "staggerMax": 100
        },
        "fume": {
            "hidePrintSymbols": false,
            "disableGeometricBackground": true,
            "backgroundObjectOpacity": 0.5,
            "textHoldRatio": 1.0,
            "cameraTrackingMode": "smooth",
            "cameraSpeed": 1.0,
            "glowIntensity": 1.0,
            "heroScale": 1.0
        },
        "claddagh": {
            "focusScaleRatio": 0.65,
            "radiusScale": 1.0,
            "ellipseTiltDeg": 45,
            "showAxisLine": true,
            "letterSpacingOffset": 0.0
        },
        "cappella": {
            "showEmoMessages": true,
            "emojiPackSource": "builtin",
            "avatarSource": "cover"
        },
        "tilt": {
            "splitProbability": 0.75,
            "tiltStyleProbability": 0.35,
            "colorScheme": "default"
        },
        "diorama": {
            "cameraSpeed": 1.0,
            "motionAmount": 1.0,
            "audioReactivity": 1.0,
            "geometryVisibility": {
                "enabled": true,
                "mode": "clouds",
                "strands": true,
                "blobs": true,
                "ribbons": true,
                "rings": true
            },
            "particleDensity": 576,
            "particleScale": 1.0,
            "particleGlowEnabled": true,
            "particleGlowIntensity": 0.65,
            "showParticles": true,
            "backgroundParticleCircumference": 28,
            "backgroundParticleRadial": 2,
            "glowEnabled": true,
            "glowIntensity": 1.0,
            "soulEnabled": true,
            "soulIntensity": 1.0,
            "soulActiveEnabled": false,
            "gradientEnabled": false,
            "gradientIntensity": 1.0,
            "keywordColoringEnabled": true
        },
        "pendolo": {
            "arcRadius": 0.42,
            "arcAngleDeg": 100,
            "wheelCenterX": 0.0,
            "wheelCenterY": 0.50,
            "tickSnappiness": 2.0,
            "activeScale": 1.25,
            "showGearDecor": "subtle",
            "showCenterGradient": true,
            "showCoverOnWatchFace": false,
            "enableLineGlow": false
        },
        "monet": {
            "keywordColoringEnabled": true,
            "showDescription": true,
            "audioStyle": "bar",
            "fontScale": 1.2,
            "portraitSource": "cover",
            "portraitOffsetX": 0,
            "portraitStyle": "square",
            "showPortraitDragHanger": true
        },
        "sonnet": {
            "cameraIntensity": 1.0,
            "typographyMotion": 1.0,
            "mgDensity": 1.0,
            "showOnlyText": false,
            "showGuide": true,
            "showBackgroundMg": true,
            "showFixedGeo": true,
            "showGiantDecorativeText": true,
            "showBackgroundDecor": true,
            "enableTransitions": true,
            "outerFrameMode": "full",
            "textureResolution": 1.5,
            "postProcessEnabled": false,
            "postProcessGrain": 0.2,
            "postProcessContrast": 0.0,
            "postProcessRgbShift": 0.0,
            "postProcessHalftone": 0.0,
            "postProcessVignette": 0.85,
            "postProcessLensDistortion": 0.3,
            "postProcessLensDispersion": 0.6
        }
    })
}

/// 全新、自洽的配置（首次启动自动生成）。
/// 单个 `默认` 预设，enabled=true，mode=sonnet，tuning=全默认 bundle。
pub fn default_config() -> Value {
    json!({
        "activePreset": DEFAULT_PRESET,
        "presets": {
            DEFAULT_PRESET: {
                "enabled": true,
                "visualizerMode": "sonnet",
                "foliaTuning": default_bundle()
            }
        }
    })
}

/// 读取并解析配置文件；文件缺失或损坏时写入默认配置并返回之（"启动时自动生成"）。
pub fn load() -> Value {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            log::warn!("folia-lyrics.json 解析失败 ({e})；重写默认");
            let cfg = default_config();
            let _ = save(&cfg);
            cfg
        }),
        Err(_) => {
            log::info!("folia-lyrics.json 不存在；自动生成默认配置 → {}", path.display());
            let cfg = default_config();
            if let Err(e) = save(&cfg) {
                log::warn!("写默认 folia-lyrics.json 失败: {e}");
            }
            cfg
        }
    }
}

/// 原子写入配置（先写临时文件再 rename，避免半写入）。
pub fn save(cfg: &Value) -> std::io::Result<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// 深度合并 `base` 与 `override`；`override` 的键优先。用于"用户改了部分字段，其余回落默认"。
fn deep_merge(base: &mut Value, override_v: &Value) {
    // 两个都是对象 → 递归按键合并（override 的键覆盖、新增；其余回落 base）。
    if let Value::Object(base_obj) = base {
        if let Value::Object(over_obj) = override_v {
            for (k, v) in over_obj.iter() {
                match base_obj.get_mut(k) {
                    Some(existing) => deep_merge(existing, v),
                    None => {
                        base_obj.insert(k.clone(), v.clone());
                    }
                }
            }
            return;
        }
    }
    // 不是两者都是对象：override 非 null 则整体覆盖（null = 缺省，保留 base）。
    if !override_v.is_null() {
        *base = override_v.clone();
    }
}

/// 取激活预设（返回其 visualizerMode + foliaTuning + enabled）。
/// 若 activePreset 缺失或不存在，回退到第一个预设或全默认。
pub fn active_preset(cfg: &Value) -> Value {
    let presets = cfg.get("presets").and_then(|p| p.as_object());
    let presets = match presets {
        Some(p) if !p.is_empty() => p,
        _ => return default_config().get("presets").cloned().unwrap().get(DEFAULT_PRESET).cloned().unwrap(),
    };
    let active_name = cfg.get("activePreset").and_then(|a| a.as_str()).unwrap_or("");
    if let Some(p) = presets.get(active_name) {
        return p.clone();
    }
    // activePreset 指向不存在的预设 → 取第一个
    presets.iter().next().map(|(_, v)| v.clone()).unwrap()
}

/// 把激活预设的 `visualizerMode` 与 `foliaTuning` 合并进 wallpaper pack 的 params JSON。
///
/// `pack_params_json`：resolve_wallpaper 给的 project.json `params`（至少含 `visualizerMode`）。
/// 返回：合并后的 JSON 字符串，直接喂 `WebWallpaperPlayer::send_config`。
///   - visualizerMode：预设有的覆盖 pack 的；都没有则回退 "classic"
///   - foliaTuning：预设的 tuning 与默认 bundle 做深度合并（保证缺失字段有默认值）
///   - 预设 enabled=false 时不合并 tuning（且不覆盖 visualizerMode），等同于"暂停歌词可视化"
pub fn merge_config_payload(pack_params_json: &str) -> String {
    let mut params: Value = serde_json::from_str(pack_params_json)
        .unwrap_or_else(|_| json!({}));
    if !params.is_object() {
        params = json!({});
    }
    let cfg = load();
    let preset = active_preset(&cfg);
    let enabled = preset.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    if enabled {
        // visualizerMode：预设优先
        if let Some(m) = preset.get("visualizerMode").and_then(|v| v.as_str()) {
            if !m.is_empty() {
                params["visualizerMode"] = json!(m);
            }
        }
        // foliaTuning：合并默认 + 预设（补齐缺失字段）
        let mut bundle = default_bundle();
        if let Some(t) = preset.get("foliaTuning") {
            deep_merge(&mut bundle, t);
        }
        params["foliaTuning"] = bundle;
    }
    serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_one_preset_and_all_modes() {
        let cfg = default_config();
        assert_eq!(cfg["activePreset"], DEFAULT_PRESET);
        let p = &cfg["presets"][DEFAULT_PRESET];
        assert_eq!(p["enabled"], true);
        assert_eq!(p["visualizerMode"], "sonnet");
        let t = &p["foliaTuning"];
        for m in MODES.iter() {
            assert!(t.get(m).is_some(), "默认 bundle 缺模式 {m}");
        }
    }

    #[test]
    fn deep_merge_preserves_unspecified_defaults() {
        let mut base = json!({ "a": 1, "b": { "x": 1 } });
        let over = json!({ "b": { "y": 2 } });
        deep_merge(&mut base, &over);
        assert_eq!(base["a"], 1);
        assert_eq!(base["b"]["x"], 1);
        assert_eq!(base["b"]["y"], 2);
    }

    #[test]
    fn active_preset_falls_back_when_missing() {
        let cfg = json!({ "activePreset": "不存在", "presets": { "其他": { "visualizerMode": "classic" } } });
        let p = active_preset(&cfg);
        assert_eq!(p["visualizerMode"], "classic");
    }

    #[test]
    fn merge_picks_preset_mode_over_pack() {
        let cfg = json!({
            "activePreset": "P1",
            "presets": { "P1": { "enabled": true, "visualizerMode": "fume",
                                 "foliaTuning": { "fume": { "cameraSpeed": 2.5 } } } }
        });
        // 模拟 load()：直接测内部逻辑（不落盘）
        let preset = active_preset(&cfg);
        let mut params: Value = serde_json::from_str(r#"{"visualizerMode":"sonnet"}"#).unwrap();
        let enabled = preset["enabled"].as_bool().unwrap_or(true);
        if enabled {
            if let Some(m) = preset["visualizerMode"].as_str() { params["visualizerMode"] = json!(m); }
            let mut bundle = default_bundle();
            if let Some(t) = preset.get("foliaTuning") { deep_merge(&mut bundle, t); }
            params["foliaTuning"] = bundle;
        }
        assert_eq!(params["visualizerMode"], "fume");            // 预设覆盖 pack
        assert_eq!(params["foliaTuning"]["fume"]["cameraSpeed"], 2.5); // 用户值生效
        assert_eq!(params["foliaTuning"]["fume"]["heroScale"], 1.0);   // 其余回落默认
        assert_eq!(params["foliaTuning"]["sonnet"]["outerFrameMode"], "full"); // 其它模式默认仍在
    }

    #[test]
    fn disabled_preset_skips_merge() {
        let cfg = json!({
            "activePreset": "P1",
            "presets": { "P1": { "enabled": false, "visualizerMode": "fume" } }
        });
        let preset = active_preset(&cfg);
        let mut params: Value = serde_json::from_str(r#"{"visualizerMode":"sonnet"}"#).unwrap();
        let enabled = preset["enabled"].as_bool().unwrap_or(true);
        if enabled {
            if let Some(m) = preset["visualizerMode"].as_str() { params["visualizerMode"] = json!(m); }
            params["foliaTuning"] = default_bundle();
        }
        assert_eq!(params["visualizerMode"], "sonnet");   // 不覆盖
        assert!(params.get("foliaTuning").is_none());      // 不注入 tuning
    }
}
