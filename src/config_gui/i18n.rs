// i18n — 双语字符串表（中/英）。
//
// Tr::get(lang, key) 返回对应语言的字符串。key 命名空间用 `.` 分隔。
// 当前覆盖 GUI 必需的所有键。新增字段时同步往 table 加条目。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_str(s: &str) -> Self {
        if s.starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }
    pub fn other(self) -> Self {
        match self {
            Lang::Zh => Lang::En,
            Lang::En => Lang::Zh,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Tr;

impl Tr {
    pub fn new() -> Self {
        Tr
    }
    pub fn get<'a>(&self, lang: Lang, key: &'a str) -> &'a str {
        match lang {
            Lang::Zh => zh(key).unwrap_or(key),
            Lang::En => en(key).unwrap_or(key),
        }
    }
}

fn zh<'a>(k: &'a str) -> Option<&'a str> {
    let table: &[(&str, &str)] = &[
        ("app.title", "pulse-ring 配置"),
        // tabs
        ("tab.shape", "形状 & 颜色"),
        ("tab.rings", "三层环"),
        ("tab.spawn", "出生动画"),
        ("tab.audio", "音频 & 位置"),
        ("tab.language", "语言"),
        ("tab.particles", "粒子"),
        ("tab.wallpaper", "壁纸"),
        ("tab.widgets", "挂件"),
        ("tab.lyric", "歌词可视化"),
        // shape
        ("shape.shape", "形状"),
        ("shape.ring", "环"),
        ("shape.square", "方"),
        ("shape.diamond", "菱形"),
        ("shape.hexagon", "六边形"),
        ("shape.triangle", "三角"),
        ("shape.star", "星形"),
        ("shape.flower", "花瓣"),
        ("shape.colorMode", "配色模式"),
        ("shape.colorMode.hue", "色相流动"),
        ("shape.colorMode.solid", "纯色"),
        ("shape.colorMode.gradient", "渐变"),
        ("shape.corners", "角数"),
        ("shape.spikiness", "尖刺"),
        ("shape.rotate", "旋转"),
        ("shape.autoRotate", "自旋速度"),
        ("shape.ringWidth", "环宽"),
        ("shape.baseRadius", "基础半径"),
        ("shape.growth", "生长"),
        ("shape.haloStrength", "光晕强度"),
        ("shape.haloSize", "光晕尺寸"),
        ("shape.alpha", "透明度"),
        ("shape.outerUniform", "外圈均匀"),
        // rings
        ("rings.inner", "内环"),
        ("rings.enable", "启用"),
        ("rings.radius", "半径"),
        ("rings.growth", "生长"),
        ("rings.width", "宽"),
        ("rings.alpha", "透明度"),
        ("rings.mid", "中环"),
        ("rings.saturn", "土星带"),
        ("rings.saturnBand", "带宽"),
        ("rings.saturnStripes", "条纹"),
        // spawn
        ("spawn.effect", "出场效果"),
        ("spawn.effect.note", "注：magic 当前被上游解析映射到 expand"),
        ("spawn.effect.none", "无"),
        ("spawn.effect.expand", "展开"),
        ("spawn.effect.zoom", "缩放"),
        ("spawn.effect.magic", "魔法（折叠到展开）"),
        ("spawn.duration", "持续 (ms)"),
        ("spawn.ease", "缓动"),
        ("spawn.ease.outCubic", "OutCubic"),
        ("spawn.ease.outBack", "OutBack"),
        ("spawn.ease.elastic", "Elastic"),
        ("spawn.ease.bounce", "Bounce"),
        ("spawn.rotate", "旋转角度"),
        // audio
        ("audio.sensitivity", "灵敏度"),
        ("audio.decay", "衰减"),
        ("audio.smoothness", "平滑度"),
        ("audio.idleBreathe", "空闲呼吸"),
        ("audio.xOffset", "横向偏移"),
        ("audio.yOffset", "纵向偏移"),
        // lang
        ("lang.choose", "选择界面语言"),
        ("lang.note", "切换后需重启 GUI 生效"),
        // common
        ("common.save", "保存"),
        ("common.savedHint", "已保存到 ~/.config/pulse-ring/pulse-ring.qml"),
    ];
    table.iter().find(|(k2, _)| *k2 == k).map(|(_, v)| *v)
}

fn en<'a>(k: &'a str) -> Option<&'a str> {
    let table: &[(&str, &str)] = &[
        ("app.title", "pulse-ring Settings"),
        ("tab.shape", "Shape & Color"),
        ("tab.rings", "Three Rings"),
        ("tab.spawn", "Spawn Animation"),
        ("tab.audio", "Audio & Position"),
        ("tab.language", "Language"),
        ("tab.particles", "Particles"),
        ("tab.wallpaper", "Wallpaper"),
        ("tab.widgets", "Widgets"),
        ("tab.lyric", "Lyric Visualizer"),
        ("shape.shape", "Shape"),
        ("shape.ring", "Ring"),
        ("shape.square", "Square"),
        ("shape.diamond", "Diamond"),
        ("shape.hexagon", "Hexagon"),
        ("shape.triangle", "Triangle"),
        ("shape.star", "Star"),
        ("shape.flower", "Flower"),
        ("shape.colorMode", "Color Mode"),
        ("shape.colorMode.hue", "Hue Flow"),
        ("shape.colorMode.solid", "Solid"),
        ("shape.colorMode.gradient", "Gradient"),
        ("shape.corners", "Corners"),
        ("shape.spikiness", "Spikiness"),
        ("shape.rotate", "Rotate"),
        ("shape.autoRotate", "Auto-rotate"),
        ("shape.ringWidth", "Ring width"),
        ("shape.baseRadius", "Base radius"),
        ("shape.growth", "Growth"),
        ("shape.haloStrength", "Halo strength"),
        ("shape.haloSize", "Halo size"),
        ("shape.alpha", "Alpha"),
        ("shape.outerUniform", "Outer uniform"),
        ("rings.inner", "Inner ring"),
        ("rings.enable", "Enable"),
        ("rings.radius", "Radius"),
        ("rings.growth", "Growth"),
        ("rings.width", "Width"),
        ("rings.alpha", "Alpha"),
        ("rings.mid", "Mid ring"),
        ("rings.saturn", "Saturn band"),
        ("rings.saturnBand", "Band width"),
        ("rings.saturnStripes", "Stripes"),
        ("spawn.effect", "Spawn effect"),
        ("spawn.effect.note", "Note: magic is collapsed to expand by upstream parser"),
        ("spawn.effect.none", "None"),
        ("spawn.effect.expand", "Expand"),
        ("spawn.effect.zoom", "Zoom"),
        ("spawn.effect.magic", "Magic (→Expand)"),
        ("spawn.duration", "Duration (ms)"),
        ("spawn.ease", "Easing"),
        ("spawn.ease.outCubic", "OutCubic"),
        ("spawn.ease.outBack", "OutBack"),
        ("spawn.ease.elastic", "Elastic"),
        ("spawn.ease.bounce", "Bounce"),
        ("spawn.rotate", "Rotate (deg)"),
        ("audio.sensitivity", "Sensitivity"),
        ("audio.decay", "Decay"),
        ("audio.smoothness", "Smoothness"),
        ("audio.idleBreathe", "Idle breathe"),
        ("audio.xOffset", "X offset"),
        ("audio.yOffset", "Y offset"),
        ("lang.choose", "Choose interface language"),
        ("lang.note", "Takes effect after restarting the GUI"),
        ("common.save", "Save"),
        ("common.savedHint", "Saved to ~/.config/pulse-ring/pulse-ring.qml"),
    ];
    table.iter().find(|(k2, _)| *k2 == k).map(|(_, v)| *v)
}
