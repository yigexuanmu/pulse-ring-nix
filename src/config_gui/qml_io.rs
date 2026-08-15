// qml_io — 读写 pulse-ring.qml。
//
// 加载: 复用 config::Config::load(path)（上游公开），失败时退回 Config::default()。
// 保存: 手写 QML 文本序列化器。这是 v1 方案（全量重写，丢失原文件注释）；
// 生成的 QML 必须能被上游 parser 完整读回（round-trip），这是这套实现的核心契约。
//
// 说明：上游 parser 接受的值格式（颜色 `#RRGGBB` 或 `#AARRGGBB`、布尔 true/false、
// 数组 [a, b, c]、嵌套 Type { key: value }）已被本序列化器严格对齐。

use std::path::Path;
use crate::config::{
    self, ColorMode, ParticleMode, ParticleShape, Shape, SpawnEffect, SpawnEase, WallpaperMode,
    WidgetType,
};

/// 加载配置（读 ~/.config/pulse-ring/pulse-ring.qml 或 PULSE_RING_CONFIG）。
pub fn load_config() -> config::Config {
    let p = config::config_path();
    if p.exists() {
        config::Config::load(&p)
    } else {
        config::Config::default()
    }
}

/// 保存配置到默认 qml 路径（$XDG_CONFIG_HOME/pulse-ring/pulse-ring.qml）。
pub fn save_config(cfg: &config::Config) -> std::io::Result<()> {
    let p = config::config_path();
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let text = serialize(cfg);
    std::fs::write(&p, text)
}

/// 把 Config 序列化回 pulse-ring.qml 文本。
///
/// 采用"全量重写"打法：丢原文件注释，输出格式化多行 QML。
/// 默认值也照实输出 — 保证 round-trip 不丢字段。
pub fn serialize(cfg: &config::Config) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("// pulse-ring 配置 — 由 pulse-ring-config GUI 生成\n");
    s.push_str("// 手动修改请保留本文件结构；GUI 会全量重写本文件。\n");
    s.push_str("PulseRing {\n");

    // ---- shape / color ----
    kv(&mut s, "shape", shape_str(cfg.shape));
    kv(&mut s, "corners", num(cfg.corners));
    kv(&mut s, "spikiness", num(cfg.spikiness));
    kv(&mut s, "rotate", num(cfg.rotate));
    kv(&mut s, "autoRotate", num(cfg.auto_rotate));
    kv(&mut s, "colorMode", color_mode_str(cfg.color_mode));
    kv(&mut s, "colors", color_array(&cfg.colors));
    kv(&mut s, "ringWidth", num(cfg.ring_width));
    kv(&mut s, "baseRadius", num(cfg.base_radius));
    kv(&mut s, "growth", num(cfg.growth));
    kv(&mut s, "outerUniform", bool_s(cfg.outer_uniform));
    kv(&mut s, "haloStrength", num(cfg.halo_strength));
    kv(&mut s, "haloSize", num(cfg.halo_size));
    kv(&mut s, "alpha", num(cfg.alpha));
    kv(&mut s, "renderScale", num(cfg.render_scale));
    kv(&mut s, "renderScreen", int(cfg.render_screen));
    kv(&mut s, "dashCount", num(cfg.dash_count));
    kv(&mut s, "dashRatio", num(cfg.dash_ratio));

    // ---- three rings ----
    kv(&mut s, "innerRing", bool_s(cfg.inner_ring));
    kv(&mut s, "innerRadius", num(cfg.inner_radius));
    kv(&mut s, "innerGrowth", num(cfg.inner_growth));
    kv(&mut s, "innerWidth", num(cfg.inner_width));
    kv(&mut s, "innerColor", color(&cfg.inner_color));
    kv(&mut s, "innerAlpha", num(cfg.inner_alpha));
    kv(&mut s, "midRing", bool_s(cfg.mid_ring));
    kv(&mut s, "midRadius", num(cfg.mid_radius));
    kv(&mut s, "midGrowth", num(cfg.mid_growth));
    kv(&mut s, "midWidth", num(cfg.mid_width));
    kv(&mut s, "midColor", color(&cfg.mid_color));
    kv(&mut s, "saturnBand", num(cfg.saturn_band));
    kv(&mut s, "saturnAlpha", num(cfg.saturn_alpha));
    kv(&mut s, "saturnStripes", num(cfg.saturn_stripes));

    // ---- spawn ----
    kv(&mut s, "spawnEffect", spawn_effect_str(cfg.spawn_effect));
    kv(&mut s, "spawnDuration", num(cfg.spawn_duration));
    kv(&mut s, "spawnEase", spawn_ease_str(cfg.spawn_ease));
    kv(&mut s, "spawnRotate", num(cfg.spawn_rotate));

    // ---- particles ----
    kv(&mut s, "particleShape", particle_shape_str(cfg.particle_shape));
    kv(&mut s, "particleMode", particle_mode_str(cfg.particle_mode));
    kv(&mut s, "particleLoop", bool_s(cfg.particle_loop));
    if !cfg.particles.is_empty() {
        s.push_str("    particles: [\n");
        for (i, p) in cfg.particles.iter().enumerate() {
            s.push_str("        Particle {\n");
            kv(&mut s, "x", num(p.x));
            kv(&mut s, "y", num(p.y));
            kv(&mut s, "angle", num(p.angle));
            kv(&mut s, "speed", num(p.speed));
            kv(&mut s, "size", num(p.size));
            kv(&mut s, "color", color(&p.color));
            kv(&mut s, "life", num(p.life));
            kv(&mut s, "delay", num(p.delay));
            kv(&mut s, "gravity", num(p.gravity));
            kv(&mut s, "drag", num(p.drag));
            kv(&mut s, "fadeIn", num(p.fade_in));
            kv(&mut s, "sizeEnd", num(p.size_end));
            kv(&mut s, "twinkle", num(p.twinkle));
            kv(&mut s, "wave", num(p.wave));
            kv(&mut s, "spinSpeed", num(p.spin_speed));
            s.push_str("        }");
            if i + 1 < cfg.particles.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("    ]\n");
    }

    // ---- wallpaper ----
    opt_str(&mut s, "imageWallpaper", &cfg.image_wallpaper);
    kv(&mut s, "imageWallpaperMode", wallpaper_mode_str(cfg.image_wallpaper_mode));
    opt_str(&mut s, "videoWallpaper", &cfg.video_wallpaper);
    kv(&mut s, "videoWallpaperAudio", bool_s(cfg.video_wallpaper_audio));
    opt_str(&mut s, "webWallpaper", &cfg.web_wallpaper);
    opt_str(&mut s, "sceneWallpaper", &cfg.scene_wallpaper);
    if !cfg.wallpapers.is_empty() {
        s.push_str("    wallpapers: [\n");
        for (i, w) in cfg.wallpapers.iter().enumerate() {
            s.push_str("        ");
            s.push_str(&qstr(w));
            if i + 1 < cfg.wallpapers.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("    ]\n");
    }
    kv(&mut s, "wallpaperInterval", num(cfg.wallpaper_interval));
    kv(&mut s, "wallpaperTransition", num(cfg.wallpaper_transition));
    kv(&mut s, "wallpaperTransitionEffect", qstr(&cfg.wallpaper_transition_effect));
    opt_str(&mut s, "luaScript", &cfg.lua_script);

    // ---- widgets ----
    if !cfg.widgets.is_empty() {
        s.push_str("    widgets: [\n");
        for (i, w) in cfg.widgets.iter().enumerate() {
            s.push_str("        Widget {\n");
            kv(&mut s, "type", widget_type_str(w.widget_type));
            kv(&mut s, "x", num(w.x));
            kv(&mut s, "y", num(w.y));
            kv(&mut s, "size", num(w.size));
            kv(&mut s, "alpha", num(w.alpha));
            kv(&mut s, "rotate", num(w.rotate));
            if let Some(src) = &w.source {
                kv(&mut s, "source", qstr(src));
            }
            kv(&mut s, "color", color(&w.color));
            kv(&mut s, "fontSize", num(w.font_size));
            kv(&mut s, "shape", shape_str(w.shape));
            s.push_str("        }");
            if i + 1 < cfg.widgets.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("    ]\n");
    }

    // ---- audio / position ----
    kv(&mut s, "idleBreathe", num(cfg.idle_breathe));
    kv(&mut s, "sensitivity", num(cfg.sensitivity));
    kv(&mut s, "decay", num(cfg.decay));
    kv(&mut s, "smoothness", num(cfg.smoothness));
    kv(&mut s, "xOffset", num(cfg.x_offset));
    kv(&mut s, "yOffset", num(cfg.y_offset));

    s.push_str("}\n");
    s
}

// ---------- helpers ----------

fn kv(s: &mut String, k: &str, v: String) {
    s.push_str("    ");
    s.push_str(k);
    s.push_str(": ");
    s.push_str(&v);
    s.push('\n');
}

fn num(x: f32) -> String {
    // 整数值输出为整数 (e.g. 5 not 5.0)，匹配 QML 习惯
    if x.fract() == 0.0 {
        format!("{}", x as i64)
    } else {
        format!("{:.4}", x).trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn int(x: i32) -> String { x.to_string() }

fn bool_s(b: bool) -> String { (if b { "true" } else { "false" }).into() }

fn qstr(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }

fn opt_str(s: &mut String, k: &str, v: &Option<String>) {
    if let Some(v) = v {
        if !v.is_empty() {
            kv(s, k, qstr(v));
        }
    }
}

/// [f32;4] (r,g,b,a in 0..1) → "#RRGGBB" (alpha=1) 或 "#AARRGGBB" (alpha<1)。
/// 注意上游 8-char 格式是 ARGB（alpha 前置），匹配 parse_colour 的 [0..2]=a 分支。
fn color(c: &[f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = (c[3].clamp(0.0, 1.0) * 255.0).round() as u8;
    if a == 255 {
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    } else {
        // 上游 8-char: AARRGGBB
        format!("#{:02X}{:02X}{:02X}{:02X}", a, r, g, b)
    }
}

fn color_array(cs: &[[f32; 4]]) -> String {
    let parts: Vec<String> = cs.iter().map(color).collect();
    format!("[{}]", parts.join(", "))
}

fn shape_str(s: Shape) -> String {
    match s {
        Shape::Ring => "ring",
        Shape::Square => "square",
        Shape::Diamond => "diamond",
        Shape::Hexagon => "hexagon",
        Shape::Triangle => "triangle",
        Shape::Star => "star",
        Shape::Flower => "flower",
    }
    .into()
}

fn color_mode_str(c: ColorMode) -> String {
    match c {
        ColorMode::Hue => "hue",
        ColorMode::Solid => "solid",
        ColorMode::Gradient => "gradient",
    }
    .into()
}

fn spawn_effect_str(e: SpawnEffect) -> String {
    match e {
        SpawnEffect::Expand => "expand",
        SpawnEffect::Zoom => "zoom",
        SpawnEffect::Magic => "magic",
        SpawnEffect::None => "none",
    }
    .into()
}

fn spawn_ease_str(e: SpawnEase) -> String {
    match e {
        SpawnEase::OutCubic => "outCubic",
        SpawnEase::OutBack => "outBack",
        SpawnEase::Elastic => "elastic",
        SpawnEase::Bounce => "bounce",
    }
    .into()
}

fn particle_shape_str(p: ParticleShape) -> String {
    match p {
        ParticleShape::Circle => "circle",
        ParticleShape::Square => "square",
        ParticleShape::Diamond => "diamond",
        ParticleShape::Star => "star",
    }
    .into()
}

fn particle_mode_str(p: ParticleMode) -> String {
    match p {
        ParticleMode::Burst => "burst",
        ParticleMode::Orbit => "orbit",
        ParticleMode::Ring => "ring",
        ParticleMode::None => "none",
    }
    .into()
}

fn wallpaper_mode_str(m: WallpaperMode) -> String {
    match m {
        WallpaperMode::Cover => "cover",
        WallpaperMode::Contain => "contain",
        WallpaperMode::Stretch => "stretch",
    }
    .into()
}

fn widget_type_str(t: WidgetType) -> String {
    use config::WidgetType as W;
    match t {
        W::Ring => "ring",
        W::Image => "image",
        W::Clock => "clock",
        W::Bars => "bars",
        W::Cover => "cover",
        W::Analog => "analog",
        W::Plugin => "plugin",
        W::Lyric => "lyric",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_default_config() {
        let cfg = config::Config::default();
        let text = serialize(&cfg);
        // 写回的文本必须能被上游 parser 读回，且关键字段保持
        let parsed = config::parse_for_test(&text);
        assert_eq!(parsed.shape, cfg.shape);
        assert_eq!(parsed.corners, cfg.corners);
        assert_eq!(parsed.color_mode, cfg.color_mode);
        assert_eq!(parsed.colors.len(), cfg.colors.len());
        assert_eq!(parsed.inner_ring, cfg.inner_ring);
        assert_eq!(parsed.ring_width, cfg.ring_width);
        assert_eq!(parsed.base_radius, cfg.base_radius);
        assert_eq!(parsed.spawn_effect, cfg.spawn_effect);
        assert_eq!(parsed.spawn_ease, cfg.spawn_ease);
        assert_eq!(parsed.sensitivity, cfg.sensitivity);
        assert_eq!(parsed.image_wallpaper_mode, cfg.image_wallpaper_mode);
    }

    #[test]
    fn color_formats_match_parser() {
        // 8-char ARGB: parser 取 [0..2]=a, [2..4]=r, [4..6]=g, [6..8]=b
        // serialize 用 "#AARRGGBB" — alpha=半透明写 ARGB，不透明写 RRGGBB
        let c = color(&[1.0, 0.0, 0.0, 0.5]); // r=1,g=0,b=0,a=0.5
        assert!(c.starts_with('#'));
        assert_eq!(c.len(), 9); // #AARRGGBB = 9 chars
        // 不透明: 6 字符
        let c2 = color(&[0.0, 0.0, 1.0, 1.0]); // 纯蓝不透明
        assert_eq!(c2, "#0000FF");
    }

    #[test]
    fn num_integer_no_fraction() {
        assert_eq!(num(5.0), "5");
        assert_eq!(num(0.13), "0.13");
        assert_eq!(num(1800.0), "1800");
    }
}
