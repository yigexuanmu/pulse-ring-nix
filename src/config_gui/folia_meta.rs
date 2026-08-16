// folia_meta — folia 11 模式 Tuning 字段元数据（GUI 动态渲染用）。
//
// 直接照搬 folia 上游 SettingsPanel 的控件类型 + 范围 + 枚举值（见 folia-major
// `src/components/visualizer/*/Settings*.tsx` 与 `types.ts` 的 `DEFAULT_*_TUNING`）。
// 标签使用 folia `locales/{zh-CN,en}.ts` 的真实双语字符串。
//
// Cadenza 模式：folia 上游无 SettingsPanel，故此表也不暴露它，忠实"抄 folia"。
// Pendolo `wheelCenterY`、Monet `portraitOffsetX`：folia 面板没暴露给用户调整，
// 这里也跟随，不暴露。

use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Kind {
    Bool,
    /// 两值布尔，用自定义 On/Off 标签（不同于 Bool 默认的 Enable/Disable）。
    BoolOnOff,
    BoolShowHide,
    BoolOnOffZhSubtle,
    Float { min: f64, max: f64, step: f64 },
    /// 字符串枚举：每个值配双语标签。
    Enum { opts: &'static [Opt] },
}

#[derive(Clone, Copy)]
pub struct Opt {
    pub v: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
}

#[derive(Clone, Copy)]
pub struct Field {
    pub mode: &'static str,
    /// JSON 路径片段（相对 `foliaTuning.<mode>`）；点分。例如
    /// "arcRadius" 或嵌套 "geometryVisibility.enabled"。
    pub path: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
    pub kind: Kind,
}

// —— 布尔 On/Off/ShowHide 默认标签 ——
pub fn bool_on(lang_en: bool) -> &'static str { if lang_en { "On" } else { "开启" } }
pub fn bool_off(lang_en: bool) -> &'static str { if lang_en { "Off" } else { "关闭" } }
pub fn bool_show(lang_en: bool) -> &'static str { if lang_en { "Show" } else { "显示" } }
pub fn bool_hide(lang_en: bool) -> &'static str { if lang_en { "Hide" } else { "隐藏" } }
pub fn bool_enable(lang_en: bool) -> &'static str { if lang_en { "Enable" } else { "启用" } }
pub fn bool_disable(lang_en: bool) -> &'static str { if lang_en { "Disable" } else { "关闭" } }

// —— 11 个 folia 模式（含 GUI 暴露的全部字段） ——
pub const MODES: &[&str] = &[
    "classic", "cadenza", "partita", "fume", "claddagh",
    "cappella", "tilt", "pendolo", "monet", "diorama",
    "sonnet",
];

pub const FIELDS: &[Field] = &[
    // ===== classic（流光）=====
    Field { mode: "classic", path: "enableWordRotation",
        zh: "逐字旋转", en: "Per-word Rotation", kind: Kind::BoolOnOff },
    Field { mode: "classic", path: "breathingFloatMultiplier",
        zh: "呼吸浮动范围", en: "Breathing Float Range",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "classic", path: "useLegacyLayout",
        zh: "排版模式", en: "Layout Mode",
        kind: Kind::Enum { opts: &[
            Opt { v: "false", zh: "自适应", en: "Adaptive" },
            Opt { v: "true",  zh: "旧版",   en: "Legacy" },
        ]}},
    Field { mode: "classic", path: "wordSpacing",
        zh: "单词间距", en: "Word Spacing",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },

    // ===== cadenza — folia 上游不暴露 → 跳过 =====

    // ===== partita（云阶）=====
    Field { mode: "partita", path: "showGuideLines",
        zh: "引导线", en: "Guide Lines", kind: Kind::BoolShowHide },
    Field { mode: "partita", path: "useSemanticLayout",
        zh: "语义排列 (CJK)", en: "CJK Semantic Layout",
        kind: Kind::BoolOnOff },
    Field { mode: "partita", path: "staggerMin",
        zh: "错位最小值", en: "Stagger Min",
        kind: Kind::Float { min: 0.0, max: 180.0, step: 5.0 } },
    Field { mode: "partita", path: "staggerMax",
        zh: "错位最大值", en: "Stagger Max",
        kind: Kind::Float { min: 0.0, max: 180.0, step: 5.0 } },

    // ===== fume（浮名）=====
    Field { mode: "fume", path: "hidePrintSymbols",
        zh: "隐藏打印方块", en: "Hide Print Stamp", kind: Kind::Bool },
    Field { mode: "fume", path: "disableGeometricBackground",
        zh: "通用几何图形", en: "Geometric Shapes", kind: Kind::BoolShowHide },
    Field { mode: "fume", path: "backgroundObjectOpacity",
        zh: "世界背景物体透明度", en: "World Background Object Opacity",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "fume", path: "textHoldRatio",
        zh: "文字停留比例", en: "Text Hold Ratio",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "fume", path: "cameraTrackingMode",
        zh: "摄影机追焦方式", en: "Camera Tracking",
        kind: Kind::Enum { opts: &[
            Opt { v: "stepped", zh: "定格", en: "Stepped" },
            Opt { v: "smooth",  zh: "平滑", en: "Smooth" },
        ]}},
    Field { mode: "fume", path: "cameraSpeed",
        zh: "摄影机移动速度", en: "Camera Speed",
        kind: Kind::Float { min: 0.55, max: 1.85, step: 0.05 } },
    Field { mode: "fume", path: "glowIntensity",
        zh: "当前句辉光强度", en: "Active Glow",
        kind: Kind::Float { min: 0.0, max: 1.8, step: 0.05 } },
    Field { mode: "fume", path: "heroScale",
        zh: "大标题比例", en: "Hero Scale",
        kind: Kind::Float { min: 0.82, max: 1.32, step: 0.02 } },

    // ===== claddagh（回环）=====
    Field { mode: "claddagh", path: "focusScaleRatio",
        zh: "主歌词放大倍率", en: "Active Lyric Scale Ratio",
        kind: Kind::Float { min: 0.0, max: 1.5, step: 0.05 } },
    Field { mode: "claddagh", path: "radiusScale",
        zh: "轨道半径", en: "Orbit Radius Scale",
        kind: Kind::Float { min: 0.5, max: 1.5, step: 0.05 } },
    Field { mode: "claddagh", path: "ellipseTiltDeg",
        zh: "轨道倾斜度", en: "Orbit Tilt Degree",
        kind: Kind::Float { min: 0.0, max: 60.0, step: 1.0 } },
    Field { mode: "claddagh", path: "showAxisLine",
        zh: "中间轴线", en: "Center Axis Line", kind: Kind::BoolShowHide },
    Field { mode: "claddagh", path: "letterSpacingOffset",
        zh: "字符间距", en: "Letter Spacing",
        kind: Kind::Float { min: -5.0, max: 20.0, step: 0.5 } },

    // ===== cappella（群唱）=====
    Field { mode: "cappella", path: "showEmoMessages",
        zh: "显示表情包", en: "Show emoji pack", kind: Kind::BoolShowHide },
    Field { mode: "cappella", path: "emojiPackSource",
        zh: "表情包来源", en: "Emoji source",
        kind: Kind::Enum { opts: &[
            Opt { v: "builtin", zh: "内置", en: "Built-in" },
            Opt { v: "custom",  zh: "自定义", en: "Custom" },
        ]}},
    Field { mode: "cappella", path: "avatarSource",
        zh: "头像来源", en: "Avatar source",
        kind: Kind::Enum { opts: &[
            Opt { v: "cover",  zh: "封面", en: "Cover" },
            Opt { v: "builtin", zh: "内置头像", en: "Built-in avatar" },
            Opt { v: "color",  zh: "色块", en: "Color block" },
            Opt { v: "custom", zh: "自定义", en: "Custom" },
        ]}},

    // ===== tilt（倾诉）=====
    Field { mode: "tilt", path: "colorScheme",
        zh: "配色方案", en: "Color Scheme",
        kind: Kind::Enum { opts: &[
            Opt { v: "default",   zh: "双色 1", en: "Dual 1" },
            Opt { v: "swap",      zh: "双色 2", en: "Dual 2" },
            Opt { v: "accentAll", zh: "单色 1", en: "Single 1" },
            Opt { v: "primaryAll", zh: "单色 2", en: "Single 2" },
        ]}},
    Field { mode: "tilt", path: "splitProbability",
        zh: "分行概率", en: "Split Probability",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "tilt", path: "tiltStyleProbability",
        zh: "斜体强调概率", en: "Italic Emphasis Probability",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },

    // ===== pendolo（时计）=====
    Field { mode: "pendolo", path: "wheelCenterX",
        zh: "轮盘水平位置 (0 = 左边缘)", en: "Wheel Center X (0 = Left Edge)",
        kind: Kind::Float { min: -0.20, max: 0.40, step: 0.01 } },
    Field { mode: "pendolo", path: "arcRadius",
        zh: "轮盘半径", en: "Arc Radius",
        kind: Kind::Float { min: 0.25, max: 0.80, step: 0.01 } },
    Field { mode: "pendolo", path: "arcAngleDeg",
        zh: "弧度角度", en: "Arc Angle Spread",
        kind: Kind::Float { min: 40.0, max: 160.0, step: 5.0 } },
    Field { mode: "pendolo", path: "tickSnappiness",
        zh: "擒纵咬合力度", en: "Escapement Snappiness",
        kind: Kind::Float { min: 0.5, max: 2.0, step: 0.1 } },
    Field { mode: "pendolo", path: "activeScale",
        zh: "聚焦句缩放", en: "Active Line Scale",
        kind: Kind::Float { min: 1.0, max: 1.60, step: 0.05 } },
    Field { mode: "pendolo", path: "showGearDecor",
        zh: "机械齿轮饰线", en: "Clockwork Markings",
        kind: Kind::Enum { opts: &[
            Opt { v: "none",    zh: "无",     en: "None" },
            Opt { v: "subtle",  zh: "半透明", en: "Subtle" },
            Opt { v: "full",    zh: "完整",   en: "Full" },
        ]}},
    Field { mode: "pendolo", path: "showCenterGradient",
        zh: "齿轮中央深色渐变", en: "Gear Center Dark Gradient", kind: Kind::BoolOnOff },
    Field { mode: "pendolo", path: "showCoverOnWatchFace",
        zh: "表盘显示专辑封面", en: "Show Cover on Watch Face", kind: Kind::BoolShowHide },
    Field { mode: "pendolo", path: "enableLineGlow",
        zh: "线条发光效果", en: "Line Glow", kind: Kind::BoolOnOff },

    // ===== monet（莫奈）=====
    Field { mode: "monet", path: "fontScale",
        zh: "字体缩放", en: "Font Size Scale",
        kind: Kind::Float { min: 0.7, max: 1.5, step: 0.05 } },
    Field { mode: "monet", path: "keywordColoringEnabled",
        zh: "关键字着色", en: "Keyword Coloring", kind: Kind::BoolOnOff },
    Field { mode: "monet", path: "showDescription",
        zh: "显示歌曲描述", en: "Show Song Description", kind: Kind::BoolShowHide },
    Field { mode: "monet", path: "portraitSource",
        zh: "右侧肖像来源", en: "Right Portrait Source",
        kind: Kind::Enum { opts: &[
            Opt { v: "cover",   zh: "封面",     en: "Cover" },
            Opt { v: "custom",  zh: "自定义图片", en: "Custom Image" },
        ]}},
    Field { mode: "monet", path: "portraitStyle",
        zh: "封面形状", en: "Cover Shape",
        kind: Kind::Enum { opts: &[
            Opt { v: "rectangular", zh: "长方形", en: "Rectangular" },
            Opt { v: "square",      zh: "正方形", en: "Square" },
        ]}},
    Field { mode: "monet", path: "showPortraitDragHanger",
        zh: "拖拽调整按钮", en: "Drag Hanger", kind: Kind::BoolShowHide },
    Field { mode: "monet", path: "audioStyle",
        zh: "频谱样式", en: "Audio Style",
        kind: Kind::Enum { opts: &[
            Opt { v: "bar",  zh: "柱状", en: "Bars" },
            Opt { v: "line", zh: "线条", en: "Line" },
        ]}},

    // ===== diorama（镜台）=====
    Field { mode: "diorama", path: "geometryVisibility.enabled",
        zh: "点云几何", en: "Particle Geometry", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "geometryVisibility.mode",
        zh: "几何形态", en: "Geometry Style",
        kind: Kind::Enum { opts: &[
            Opt { v: "clouds",   zh: "点云",         en: "Point Clouds" },
            Opt { v: "corridor", zh: "长廊",          en: "Corridor" },
        ]}},
    Field { mode: "diorama", path: "geometryVisibility.strands",
        zh: "点云立方体 (strands)", en: "Particle Cubes", kind: Kind::BoolShowHide },
    Field { mode: "diorama", path: "geometryVisibility.blobs",
        zh: "点云圆柱体 (blobs)", en: "Particle Cylinders", kind: Kind::BoolShowHide },
    Field { mode: "diorama", path: "geometryVisibility.ribbons",
        zh: "三角晶体 (ribbons)", en: "Triangular Crystals", kind: Kind::BoolShowHide },
    Field { mode: "diorama", path: "geometryVisibility.rings",
        zh: "点云圆环 (rings)", en: "Particle Rings", kind: Kind::BoolShowHide },
    Field { mode: "diorama", path: "cameraSpeed",
        zh: "镜头速度", en: "Camera Speed",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "diorama", path: "motionAmount",
        zh: "运动幅度", en: "Motion Amount",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "diorama", path: "audioReactivity",
        zh: "点云音频响应", en: "Particle Audio Response",
        kind: Kind::Float { min: 0.0, max: 1.5, step: 0.05 } },
    Field { mode: "diorama", path: "particleDensity",
        zh: "点云密度", en: "Point Cloud Density",
        kind: Kind::Float { min: 96.0, max: 1536.0, step: 24.0 } },
    Field { mode: "diorama", path: "particleScale",
        zh: "点云整体体积", en: "Cloud Volume",
        kind: Kind::Float { min: 0.65, max: 1.6, step: 0.05 } },
    Field { mode: "diorama", path: "particleGlowEnabled",
        zh: "点云整体辉光", en: "Cloud Aura", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "particleGlowIntensity",
        zh: "辉光强度", en: "Aura Strength",
        kind: Kind::Float { min: 0.1, max: 1.5, step: 0.05 } },
    Field { mode: "diorama", path: "showParticles",
        zh: "背景粒子", en: "Background Particles", kind: Kind::BoolShowHide },
    Field { mode: "diorama", path: "backgroundParticleCircumference",
        zh: "圆周密度", en: "Ring Density",
        kind: Kind::Float { min: 4.0, max: 48.0, step: 2.0 } },
    Field { mode: "diorama", path: "backgroundParticleRadial",
        zh: "径向密度", en: "Radial Density",
        kind: Kind::Float { min: 1.0, max: 4.0, step: 1.0 } },
    Field { mode: "diorama", path: "glowEnabled",
        zh: "普通辉光跟唱", en: "Sung Glow", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "glowIntensity",
        zh: "普通辉光强度", en: "Sung Glow Strength",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "diorama", path: "soulEnabled",
        zh: "灵魂出窍跟唱", en: "Soul Drift", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "soulIntensity",
        zh: "灵魂出窍强度", en: "Soul Drift Strength",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "diorama", path: "soulActiveEnabled",
        zh: "当前字出窍", en: "Current-Word Soul-Out", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "gradientEnabled",
        zh: "渐变跟唱", en: "Progress Gradient", kind: Kind::BoolOnOff },
    Field { mode: "diorama", path: "gradientIntensity",
        zh: "渐变跟唱强度", en: "Gradient Strength",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "diorama", path: "keywordColoringEnabled",
        zh: "关键字着色", en: "Keyword Coloring", kind: Kind::BoolOnOff },

    // ===== sonnet（商籁）=====
    Field { mode: "sonnet", path: "textureResolution",
        zh: "纹理分辨率", en: "Texture resolution",
        kind: Kind::Float { min: 0.5, max: 4.0, step: 0.25 } },
    Field { mode: "sonnet", path: "cameraIntensity",
        zh: "镜头运动强度", en: "Camera intensity",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "sonnet", path: "typographyMotion",
        zh: "文字动势", en: "Typography motion",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "sonnet", path: "mgDensity",
        zh: "背景短线密度", en: "Background line density",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "sonnet", path: "outerFrameMode",
        zh: "外层边框", en: "Outer frame",
        kind: Kind::Enum { opts: &[
            Opt { v: "none",  zh: "完全隐藏",   en: "Hidden" },
            Opt { v: "frame", zh: "仅显示框架", en: "Frame only" },
            Opt { v: "full",  zh: "完全显示",   en: "Full" },
        ]}},
    Field { mode: "sonnet", path: "showOnlyText",
        zh: "仅显示文字", en: "Text only", kind: Kind::Bool },
    Field { mode: "sonnet", path: "showGuide",
        zh: "轨迹线", en: "Guide lines", kind: Kind::BoolShowHide },
    Field { mode: "sonnet", path: "showBackgroundMg",
        zh: "主场景", en: "Main scene", kind: Kind::BoolShowHide },
    Field { mode: "sonnet", path: "showFixedGeo",
        zh: "文字浮标", en: "Text markers", kind: Kind::BoolShowHide },
    Field { mode: "sonnet", path: "showGiantDecorativeText",
        zh: "巨型装饰镂空文字", en: "Giant decorative outline text", kind: Kind::BoolShowHide },
    Field { mode: "sonnet", path: "showBackgroundDecor",
        zh: "背景装饰", en: "Background decorations", kind: Kind::BoolShowHide },
    Field { mode: "sonnet", path: "enableTransitions",
        zh: "场景转场", en: "Scene transitions", kind: Kind::BoolOnOff },
    Field { mode: "sonnet", path: "postProcessEnabled",
        zh: "整体后处理滤镜", en: "Scene post-process filter", kind: Kind::BoolOnOff },
    Field { mode: "sonnet", path: "postProcessGrain",
        zh: "胶片颗粒", en: "Film grain",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessContrast",
        zh: "对比度增强", en: "Contrast boost",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessRgbShift",
        zh: "RGB 色差", en: "RGB shift",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessHalftone",
        zh: "半调网点", en: "Halftone screen",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessVignette",
        zh: "暗角", en: "Vignette",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessLensDistortion",
        zh: "透镜扭曲", en: "Lens distortion",
        kind: Kind::Float { min: 0.0, max: 2.0, step: 0.05 } },
    Field { mode: "sonnet", path: "postProcessLensDispersion",
        zh: "透镜色散", en: "Lens dispersion",
        kind: Kind::Float { min: 0.0, max: 1.0, step: 0.05 } },
];

/// 拆分点分路径：`"geometryVisibility.enabled" → ["geometryVisibility", "enabled"]`。
fn split_path(path: &str) -> Vec<&str> {
    path.split('.').collect()
}

/// 沿路径只读取，缺失时返回 None。
pub fn get<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = value;
    for seg in split_path(path) {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// 沿路径写入（沿途创建 Object 节点）。叶覆盖（不是 null 则覆盖）。
pub fn set(value: &mut Value, path: &str, new: Value) {
    let segs = split_path(path);
    if segs.is_empty() { return; }
    let mut cur = value;
    for seg in &segs[..segs.len() - 1] {
        if !cur.is_object() {
            *cur = Value::Object(serde_json::Map::new());
        }
        let obj = cur.as_object_mut().unwrap();
        if !obj.contains_key(*seg) {
            obj.insert((*seg).to_string(), Value::Object(serde_json::Map::new()));
        }
        cur = obj.get_mut(*seg).unwrap();
    }
    let last = segs.last().unwrap();
    if !cur.is_object() {
        *cur = Value::Object(serde_json::Map::new());
    }
    cur.as_object_mut().unwrap().insert((*last).to_string(), new);
}
