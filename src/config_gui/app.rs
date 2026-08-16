// app — 主窗口构建。
//
// 持有 Rc<RefCell<GuiState>>：Config + Lang。各 PreferencesPage 用 libadwaita 行控件
// 与上游 Config 状态双向绑定。
//
// v1 语言策略：切换写偏好文件 + 弹提示「需重启 GUI 生效」，不在运行时重建窗口
// （重建涉及销毁活跃 window，逻辑复杂；交给"用户重开 GUI"这一自然动作）。

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::i18n::{Lang, Tr};
use super::qml_io;

pub struct GuiState {
    pub config: crate::config::Config,
    pub lang: Lang,
    pub tr: Tr,
    /// folia 歌词可视化配置（~/.config/pulse-ring/folia-lyrics.json，独立于 QML）。
    /// 结构：{ activePreset, presets: { <name>: { enabled, visualizerMode, foliaTuning } } }
    pub folia: serde_json::Value,
}

pub type State = Rc<RefCell<GuiState>>;

const GUI_PREF_NAME: &str = "pulse-ring-config.json";

fn pref_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
        });
    let dir = base.join("pulse-ring");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(GUI_PREF_NAME)
}

fn load_lang_pref() -> Lang {
    if let Ok(s) = std::fs::read_to_string(pref_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(l) = v.get("lang").and_then(|x| x.as_str()) {
                return Lang::from_str(l);
            }
        }
    }
    Lang::from_str(&std::env::var("LANG").unwrap_or_else(|_| "en".into()))
}

fn save_lang_pref(lang: Lang) {
    let obj = serde_json::json!({ "lang": lang.code() });
    let _ = std::fs::write(pref_path(), obj.to_string());
}

pub fn build_main_window(app: &adw::Application) {
    let state: State = Rc::new(RefCell::new(GuiState {
        config: qml_io::load_config(),
        folia: crate::folia_lyrics::load(),
        lang: load_lang_pref(),
        tr: Tr::new(),
    }));
    present_window(app, state);
}

fn present_window(app: &adw::Application, state: State) {
    let lang;
    let title;
    {
        let s = state.borrow();
        lang = s.lang;
        title = s.tr.get(lang, "app.title").to_string();
    }
    let tr = Tr::new();

    // AdwPreferencesWindow 本身是 GtkWindow（顶层 root）—— 直接当顶层窗口用，
    // 严禁再套 ApplicationWindow/ToolbarView（之前崩溃根因：双重 root）。
    let prefs = adw::PreferencesWindow::builder()
        .application(app)
        .title(&title)
        .default_width(900)
        .default_height(680)
        .build();
    prefs.set_search_enabled(true);

    // —— 各 PreferencesPage ——
    build_general_page(&prefs, state.clone(), &tr, lang);
    build_shape_color_page(&prefs, state.clone(), &tr, lang);
    build_rings_page(&prefs, state.clone(), &tr, lang);
    build_spawn_page(&prefs, state.clone(), &tr, lang);
    build_audio_page(&prefs, state.clone(), &tr, lang);
    build_language_page(&prefs, state.clone(), &tr, lang);
    build_stub_pages(&prefs, &tr, lang);
    build_folia_page(&prefs, state.clone(), &tr, lang);

    prefs.present();
}

/// 弹一个 libadwaita Toast（保存反馈等）。parent 必须是 PreferencesWindow。
fn toast(parent: &adw::PreferencesWindow, msg: &str) {
    let t = adw::Toast::new(msg);
    parent.add_toast(t);
}

// —— helpers ——

/// 构造 ActionRow + Scale 后缀，绑定到 Config 字段。返回该 row 供 add 进 group。
fn scale_row<F>(
    title: &str,
    value: f32,
    min: f64,
    max: f64,
    step: f64,
    state: &State,
    setter: F,
) -> adw::ActionRow
where
    F: Fn(&mut crate::config::Config, f32) + 'static,
{
    let row = adw::ActionRow::builder().title(title).build();
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, min, max, step);
    scale.set_draw_value(true);
    scale.set_digits(if step >= 1.0 { 0 } else if step >= 0.05 { 2 } else { 3 });
    scale.set_hexpand(true);
    scale.set_width_request(260);
    scale.set_increments(step, step * 10.0);
    scale.set_value(value as f64);
    let st = state.clone();
    scale.connect_value_changed(move |s| {
        let v = s.value() as f32;
        setter(&mut st.borrow_mut().config, v);
    });
    row.add_suffix(&scale);
    row
}

fn shape_from_index(i: u32) -> crate::config::Shape {
    match i {
        0 => crate::config::Shape::Ring,
        1 => crate::config::Shape::Square,
        2 => crate::config::Shape::Diamond,
        3 => crate::config::Shape::Hexagon,
        4 => crate::config::Shape::Triangle,
        5 => crate::config::Shape::Star,
        _ => crate::config::Shape::Flower,
    }
}

// ============== General (Save + apply hint) page ==============

fn build_general_page(prefs: &adw::PreferencesWindow, state: State, tr: &Tr, lang: Lang) {
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.general"));
    page.set_icon_name(Some("emblem-system-symbolic"));

    let grp = adw::PreferencesGroup::new();
    grp.set_title(tr.get(lang, "tab.general"));
    grp.set_description(Some(tr.get(lang, "common.applyHint")));

    // 保存按钮：AdwButtonRow（libadwaita 1.5+），看起来像按钮的整行。
    let save_row = adw::ButtonRow::builder()
        .title(tr.get(lang, "common.save"))
        .build();
    save_row.add_css_class("suggested-action");
    let st = state.clone();
    let prefs_clone = prefs.clone();
    save_row.connect_activated(move |_| {
        let msg = {
            let mut s = st.borrow_mut();
            // 同步 scene_wallpaper 跟随 folia 预设的 enabled 状态：
            // 这样无论用户本次开 GUI 是否手动触发开关，保存时状态都一致。
            // scene_wallpaper=Some("folia-lyrics") 才让 main.rs spawn 歌词层进程。
            let preset = crate::folia_lyrics::active_preset(&s.folia);
            let enabled = preset.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            s.config.scene_wallpaper = if enabled {
                Some("folia-lyrics".to_string())
            } else {
                None
            };
            let qml_ok = qml_io::save_config(&s.config);
            let folia_ok = crate::folia_lyrics::save(&s.folia);
            match (qml_ok, folia_ok) {
                (Ok(_), Ok(_)) => s.tr.get(s.lang, "common.savedHint").to_string(),
                (Err(e), _) => format!("{}: {}", s.tr.get(s.lang, "common.save"), e),
                (_, Err(e)) => format!("folia-lyrics.json: {}", e),
            }
        };
        toast(&prefs_clone, &msg);
    });
    grp.add(&save_row);

    page.add(&grp);
    prefs.add(&page);
}

// ============== 各页 ==============

fn build_shape_color_page(
    prefs: &adw::PreferencesWindow,
    state: State,
    tr: &Tr,
    lang: Lang,
) {
    let cfg;
    {
        let s = state.borrow();
        cfg = s.config.clone();
    }
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.shape"));
    page.set_icon_name(Some("applications-graphics-symbolic"));

    let grp = adw::PreferencesGroup::new();
    grp.set_title(tr.get(lang, "tab.shape"));

    // Shape ComboRow
    let shape_row = adw::ComboRow::builder().title(tr.get(lang, "shape.shape")).build();
    let model = gtk4::StringList::new(&[
        tr.get(lang, "shape.ring"),
        tr.get(lang, "shape.square"),
        tr.get(lang, "shape.diamond"),
        tr.get(lang, "shape.hexagon"),
        tr.get(lang, "shape.triangle"),
        tr.get(lang, "shape.star"),
        tr.get(lang, "shape.flower"),
    ]);
    shape_row.set_model(Some(&model));
    shape_row.set_selected(cfg.shape as u32);
    let st = state.clone();
    shape_row.connect_selected_notify(move |r| {
        st.borrow_mut().config.shape = shape_from_index(r.selected());
    });
    grp.add(&shape_row);

    let cm_row = adw::ComboRow::builder().title(tr.get(lang, "shape.colorMode")).build();
    let cm_model = gtk4::StringList::new(&[
        tr.get(lang, "shape.colorMode.hue"),
        tr.get(lang, "shape.colorMode.solid"),
        tr.get(lang, "shape.colorMode.gradient"),
    ]);
    cm_row.set_model(Some(&cm_model));
    cm_row.set_selected(cfg.color_mode as u32);
    let st = state.clone();
    cm_row.connect_selected_notify(move |r| {
        st.borrow_mut().config.color_mode = match r.selected() {
            0 => crate::config::ColorMode::Hue,
            1 => crate::config::ColorMode::Solid,
            _ => crate::config::ColorMode::Gradient,
        };
    });
    grp.add(&cm_row);

    grp.add(&scale_row(tr.get(lang, "shape.corners"), cfg.corners, 2.0, 20.0, 1.0, &state, |c, v| c.corners = v));
    grp.add(&scale_row(tr.get(lang, "shape.spikiness"), cfg.spikiness, 0.0, 1.0, 0.01, &state, |c, v| c.spikiness = v));
    grp.add(&scale_row(tr.get(lang, "shape.rotate"), cfg.rotate, -180.0, 180.0, 1.0, &state, |c, v| c.rotate = v));
    grp.add(&scale_row(tr.get(lang, "shape.autoRotate"), cfg.auto_rotate, -30.0, 30.0, 0.5, &state, |c, v| c.auto_rotate = v));
    grp.add(&scale_row(tr.get(lang, "shape.ringWidth"), cfg.ring_width, 0.5, 30.0, 0.5, &state, |c, v| c.ring_width = v));
    grp.add(&scale_row(tr.get(lang, "shape.baseRadius"), cfg.base_radius, 0.02, 0.5, 0.005, &state, |c, v| c.base_radius = v));
    grp.add(&scale_row(tr.get(lang, "shape.growth"), cfg.growth, 0.0, 0.5, 0.005, &state, |c, v| c.growth = v));
    grp.add(&scale_row(tr.get(lang, "shape.haloStrength"), cfg.halo_strength, 0.0, 1.0, 0.01, &state, |c, v| c.halo_strength = v));
    grp.add(&scale_row(tr.get(lang, "shape.haloSize"), cfg.halo_size, 0.0, 0.5, 0.005, &state, |c, v| c.halo_size = v));
    grp.add(&scale_row(tr.get(lang, "shape.alpha"), cfg.alpha, 0.0, 1.0, 0.01, &state, |c, v| c.alpha = v));

    let uni_row = adw::SwitchRow::builder().title(tr.get(lang, "shape.outerUniform")).build();
    uni_row.set_active(cfg.outer_uniform);
    let st = state.clone();
    uni_row.connect_active_notify(move |r| st.borrow_mut().config.outer_uniform = r.is_active());
    grp.add(&uni_row);

    page.add(&grp);
    prefs.add(&page);
}

fn build_rings_page(
    prefs: &adw::PreferencesWindow,
    state: State,
    tr: &Tr,
    lang: Lang,
) {
    let cfg;
    {
        let s = state.borrow();
        cfg = s.config.clone();
    }
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.rings"));
    page.set_icon_name(Some("view-paged-symbolic"));

    let inner_grp = adw::PreferencesGroup::new();
    inner_grp.set_title(tr.get(lang, "rings.inner"));
    let inner_switch = adw::SwitchRow::builder().title(tr.get(lang, "rings.enable")).build();
    inner_switch.set_active(cfg.inner_ring);
    let st = state.clone();
    inner_switch.connect_active_notify(move |r| st.borrow_mut().config.inner_ring = r.is_active());
    inner_grp.add(&inner_switch);
    inner_grp.add(&scale_row(tr.get(lang, "rings.radius"), cfg.inner_radius, 0.1, 0.95, 0.005, &state, |c, v| c.inner_radius = v));
    inner_grp.add(&scale_row(tr.get(lang, "rings.growth"), cfg.inner_growth, 0.0, 0.5, 0.005, &state, |c, v| c.inner_growth = v));
    inner_grp.add(&scale_row(tr.get(lang, "rings.width"), cfg.inner_width, 0.5, 20.0, 0.25, &state, |c, v| c.inner_width = v));
    inner_grp.add(&scale_row(tr.get(lang, "rings.alpha"), cfg.inner_alpha, 0.0, 1.0, 0.01, &state, |c, v| c.inner_alpha = v));
    page.add(&inner_grp);

    let mid_grp = adw::PreferencesGroup::new();
    mid_grp.set_title(tr.get(lang, "rings.mid"));
    let mid_switch = adw::SwitchRow::builder().title(tr.get(lang, "rings.enable")).build();
    mid_switch.set_active(cfg.mid_ring);
    let st = state.clone();
    mid_switch.connect_active_notify(move |r| st.borrow_mut().config.mid_ring = r.is_active());
    mid_grp.add(&mid_switch);
    mid_grp.add(&scale_row(tr.get(lang, "rings.radius"), cfg.mid_radius, 0.1, 0.95, 0.005, &state, |c, v| c.mid_radius = v));
    mid_grp.add(&scale_row(tr.get(lang, "rings.growth"), cfg.mid_growth, 0.0, 0.5, 0.005, &state, |c, v| c.mid_growth = v));
    mid_grp.add(&scale_row(tr.get(lang, "rings.width"), cfg.mid_width, 0.5, 20.0, 0.25, &state, |c, v| c.mid_width = v));
    page.add(&mid_grp);

    let sat_grp = adw::PreferencesGroup::new();
    sat_grp.set_title(tr.get(lang, "rings.saturn"));
    sat_grp.add(&scale_row(tr.get(lang, "rings.saturnBand"), cfg.saturn_band, 0.0, 0.2, 0.002, &state, |c, v| c.saturn_band = v));
    sat_grp.add(&scale_row(tr.get(lang, "rings.alpha"), cfg.saturn_alpha, 0.0, 1.0, 0.01, &state, |c, v| c.saturn_alpha = v));
    sat_grp.add(&scale_row(tr.get(lang, "rings.saturnStripes"), cfg.saturn_stripes, 0.0, 1.0, 0.01, &state, |c, v| c.saturn_stripes = v));
    page.add(&sat_grp);

    prefs.add(&page);
}

fn build_spawn_page(
    prefs: &adw::PreferencesWindow,
    state: State,
    tr: &Tr,
    lang: Lang,
) {
    let cfg;
    {
        let s = state.borrow();
        cfg = s.config.clone();
    }
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.spawn"));
    page.set_icon_name(Some("preferences-system-time-symbolic"));

    let grp = adw::PreferencesGroup::new();
    grp.set_title(tr.get(lang, "tab.spawn"));

    let effect_row =
        adw::ComboRow::builder().title(tr.get(lang, "spawn.effect")).subtitle(tr.get(lang, "spawn.effect.note")).build();
    let model = gtk4::StringList::new(&[
        tr.get(lang, "spawn.effect.none"),
        tr.get(lang, "spawn.effect.expand"),
        tr.get(lang, "spawn.effect.zoom"),
        tr.get(lang, "spawn.effect.magic"),
    ]);
    effect_row.set_model(Some(&model));
    effect_row.set_selected(match cfg.spawn_effect {
        crate::config::SpawnEffect::None => 0,
        crate::config::SpawnEffect::Expand => 1,
        crate::config::SpawnEffect::Zoom => 2,
        crate::config::SpawnEffect::Magic => 3,
    });
    let st = state.clone();
    effect_row.connect_selected_notify(move |r| {
        st.borrow_mut().config.spawn_effect = match r.selected() {
            0 => crate::config::SpawnEffect::None,
            1 => crate::config::SpawnEffect::Expand,
            2 => crate::config::SpawnEffect::Zoom,
            _ => crate::config::SpawnEffect::Magic,
        };
    });
    grp.add(&effect_row);

    grp.add(&scale_row(tr.get(lang, "spawn.duration"), cfg.spawn_duration, 200.0, 5000.0, 100.0, &state, |c, v| c.spawn_duration = v));

    let ease_row = adw::ComboRow::builder().title(tr.get(lang, "spawn.ease")).build();
    let ease_model = gtk4::StringList::new(&[
        tr.get(lang, "spawn.ease.outCubic"),
        tr.get(lang, "spawn.ease.outBack"),
        tr.get(lang, "spawn.ease.elastic"),
        tr.get(lang, "spawn.ease.bounce"),
    ]);
    ease_row.set_model(Some(&ease_model));
    ease_row.set_selected(match cfg.spawn_ease {
        crate::config::SpawnEase::OutCubic => 0,
        crate::config::SpawnEase::OutBack => 1,
        crate::config::SpawnEase::Elastic => 2,
        crate::config::SpawnEase::Bounce => 3,
    });
    let st = state.clone();
    ease_row.connect_selected_notify(move |r| {
        st.borrow_mut().config.spawn_ease = match r.selected() {
            0 => crate::config::SpawnEase::OutCubic,
            1 => crate::config::SpawnEase::OutBack,
            2 => crate::config::SpawnEase::Elastic,
            _ => crate::config::SpawnEase::Bounce,
        };
    });
    grp.add(&ease_row);

    grp.add(&scale_row(tr.get(lang, "spawn.rotate"), cfg.spawn_rotate, -360.0, 360.0, 1.0, &state, |c, v| c.spawn_rotate = v));

    page.add(&grp);
    prefs.add(&page);
}

fn build_audio_page(
    prefs: &adw::PreferencesWindow,
    state: State,
    tr: &Tr,
    lang: Lang,
) {
    let cfg;
    {
        let s = state.borrow();
        cfg = s.config.clone();
    }
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.audio"));
    page.set_icon_name(Some("audio-input-microphone-symbolic"));

    let grp = adw::PreferencesGroup::new();
    grp.set_title(tr.get(lang, "tab.audio"));
    grp.add(&scale_row(tr.get(lang, "audio.sensitivity"), cfg.sensitivity, 0.1, 5.0, 0.05, &state, |c, v| c.sensitivity = v));
    grp.add(&scale_row(tr.get(lang, "audio.decay"), cfg.decay, 0.5, 0.99, 0.005, &state, |c, v| c.decay = v));
    grp.add(&scale_row(tr.get(lang, "audio.smoothness"), cfg.smoothness, 0.0, 8.0, 0.1, &state, |c, v| c.smoothness = v));
    grp.add(&scale_row(tr.get(lang, "audio.idleBreathe"), cfg.idle_breathe, 0.0, 0.5, 0.005, &state, |c, v| c.idle_breathe = v));
    grp.add(&scale_row(tr.get(lang, "audio.xOffset"), cfg.x_offset, -0.5, 0.5, 0.01, &state, |c, v| c.x_offset = v));
    grp.add(&scale_row(tr.get(lang, "audio.yOffset"), cfg.y_offset, -0.5, 0.5, 0.01, &state, |c, v| c.y_offset = v));
    page.add(&grp);
    prefs.add(&page);
}

fn build_language_page(prefs: &adw::PreferencesWindow, state: State, tr: &Tr, lang: Lang) {
    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.language"));
    page.set_icon_name(Some("preferences-desktop-locale-symbolic"));

    let grp = adw::PreferencesGroup::new();
    grp.set_title(tr.get(lang, "lang.choose"));
    grp.set_description(Some(tr.get(lang, "lang.note")));

    let zh_row = adw::ActionRow::builder().title("中文").subtitle("Chinese").build();
    if lang == Lang::Zh {
        zh_row.add_suffix(&gtk4::Image::from_icon_name("emblem-ok-symbolic"));
    }
    let st = state.clone();
    zh_row.set_activatable(true);
    zh_row.connect_activated(move |_| {
        st.borrow_mut().lang = Lang::Zh;
        save_lang_pref(Lang::Zh);
    });
    grp.add(&zh_row);

    let en_row = adw::ActionRow::builder().title("English").subtitle("英文").build();
    if lang == Lang::En {
        en_row.add_suffix(&gtk4::Image::from_icon_name("emblem-ok-symbolic"));
    }
    let st = state.clone();
    en_row.set_activatable(true);
    en_row.connect_activated(move |_| {
        st.borrow_mut().lang = Lang::En;
        save_lang_pref(Lang::En);
    });
    grp.add(&en_row);

    page.add(&grp);
    prefs.add(&page);
}

fn build_stub_pages(prefs: &adw::PreferencesWindow, tr: &Tr, lang: Lang) {
    // 粒子 / 壁纸 / 挂件 三个上游 QML 概念暂保留占位（面向后续填上）。
    let stubs: [(&str, &str, &str); 3] = [
        (tr.get(lang, "tab.particles"), "preferences-desktop-effects-symbolic", "（v2 填充）"),
        (tr.get(lang, "tab.wallpaper"), "preferences-desktop-wallpaper-symbolic", "（v2 填充）"),
        (tr.get(lang, "tab.widgets"), "view-grid-symbolic", "（v2 填充）"),
    ];
    for &(title, icon, hint) in stubs.iter() {
        let page = adw::PreferencesPage::new();
        page.set_title(title);
        page.set_icon_name(Some(icon));
        let g = adw::PreferencesGroup::new();
        g.set_title(title);
        let r = adw::ActionRow::builder().title(title).subtitle(hint).build();
        g.add(&r);
        page.add(&g);
        prefs.add(&page);
    }
}

// ============== Folia 歌词可视化页（总开关 + 模式 + 歌模式动态参数） ==============

/// 调出 activePreset 字符串名（缺失时回退当前第一个预设 / DEFAULT_PRESET）。
fn folia_active_name(v: &serde_json::Value) -> String {
    if let Some(name) = v.get("activePreset").and_then(|x| x.as_str()) {
        return name.to_string();
    }
    if let Some(presets) = v.get("presets").and_then(|p| p.as_object()) {
        if let Some((k, _)) = presets.iter().next() {
            return k.clone();
        }
    }
    crate::folia_lyrics::DEFAULT_PRESET.to_string()
}

/// 读 `presets.<active>.<path>`（路径点分，如 "enabled" / "visualizerMode"）。
fn folia_preset_get<'a>(v: &'a serde_json::Value, active: &str, path: &str) -> Option<&'a serde_json::Value> {
    let preset = v.get("presets")?.get(active)?;
    super::folia_meta::get(preset, path)
}

/// 写 `presets.<active>.<path>`（路径点分）。沿途创建对象节点。
fn folia_preset_set(v: &mut serde_json::Value, active: &str, path: &str, new: serde_json::Value) {
    if v.get("presets").is_none() {
        *v = serde_json::json!({ "presets": {} });
    }
    let presets = v.get_mut("presets").unwrap().as_object_mut().unwrap();
    if !presets.contains_key(active) {
        presets.insert(active.to_string(), serde_json::json!({}));
    }
    let preset = presets.get_mut(active).unwrap();
    super::folia_meta::set(preset, path, new);
}

/// 读 `presets.<active>.foliaTuning.<mode>.<path>`（mode 为模式字段下的根段）。
fn folia_tuning_get<'a>(v: &'a serde_json::Value, active: &str, mode: &str, field_path: &str) -> Option<&'a serde_json::Value> {
    let pre = v.get("presets")?.get(active)?;
    let mode_obj = pre.get("foliaTuning")?.get(mode)?;
    super::folia_meta::get(mode_obj, field_path)
}

/// 写 `presets.<active>.foliaTuning.<mode>.<path>`。
fn folia_tuning_set(v: &mut serde_json::Value, active: &str, mode: &str, field_path: &str, new: serde_json::Value) {
    // presets.<active>.foliaTuning = {} if missing
    if !v.is_object() { *v = serde_json::json!({}); }
    let presets = v.get_mut("presets").unwrap().as_object_mut().unwrap();
    if !presets.contains_key(active) {
        presets.insert(active.to_string(), serde_json::json!({}));
    }
    let pre = presets.get_mut(active).unwrap();
    if pre.get("foliaTuning").is_none() { pre["foliaTuning"] = serde_json::json!({}); }
    let tuning = pre.get_mut("foliaTuning").unwrap();
    if tuning.get(mode).is_none() { tuning[mode] = serde_json::json!({}); }
    let mode_obj = tuning.get_mut(mode).unwrap();
    super::folia_meta::set(mode_obj, field_path, new);
}

fn build_folia_page(prefs: &adw::PreferencesWindow, state: State, tr: &Tr, lang: Lang) {
    let active;
    let enabled_now;
    let mode_now;
    {
        let s = state.borrow();
        active = folia_active_name(&s.folia);
        enabled_now = folia_preset_get(&s.folia, &active, "enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        mode_now = folia_preset_get(&s.folia, &active, "visualizerMode")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "classic".to_string());
    }
    let lang_en = lang == Lang::En;

    let page = adw::PreferencesPage::new();
    page.set_title(tr.get(lang, "tab.lyric"));
    page.set_icon_name(Some("applications-multimedia-symbolic"));

    // —— 顶部组：总开关 + 模式选择 ——
    let top_group = adw::PreferencesGroup::new();
    top_group.set_title(tr.get(lang, "folia.section.general"));

    // 总开关：SwitchRow 绑定 presets.<active>.enabled
    let enabled_row = adw::SwitchRow::builder().title(tr.get(lang, "folia.enabled")).build();
    enabled_row.set_active(enabled_now);
    let st = state.clone();
    let active_clone = active.clone();
    enabled_row.connect_active_notify(move |r| {
        let on = r.is_active();
        let mut st = st.borrow_mut();
        // 同步 folia-lyrics.json 的预设开关
        folia_preset_set(&mut st.folia, &active_clone, "enabled", serde_json::json!(on));
        // 同时与 QML 的 scene_wallpaper 联动：「开」即启用歌词层，
        // 「关」即卸下——一个开关端到端启停，不要求用户另下壁纸配置。
        st.config.scene_wallpaper = if on { Some("folia-lyrics".to_string()) } else { None };
    });
    top_group.add(&enabled_row);

    // 模式下拉：ComboRow，11 个模式 (cadenza 标注无面板)
    let mode_row = adw::ComboRow::builder().title(tr.get(lang, "folia.mode")).build();
    // 模式名列表（含 cadenza 但在说明里提示）
    let mode_keys: [&str; 11] = [
        "classic", "cadenza", "partita", "fume", "claddagh",
        "cappella", "tilt", "pendolo", "monet", "diorama", "sonnet",
    ];
    let mode_labels: Vec<String> = mode_keys.iter().map(|m| {
        let key = format!("folia.mode.{}", m);
        tr.get(lang, &key).to_string()
    }).collect();
    let labels_refs: Vec<&str> = mode_labels.iter().map(|s| s.as_str()).collect();
    let mode_model = gtk4::StringList::new(&labels_refs);
    mode_row.set_model(Some(&mode_model));
    let mode_idx = mode_keys.iter().position(|k| *k == mode_now).unwrap_or(0) as u32;
    mode_row.set_selected(mode_idx);
    top_group.add(&mode_row);

    let st_for_mode = state.clone();
    let active_clone2 = active.clone();
    let mode_keys_clone: Rc<Vec<String>> = Rc::new(mode_keys.iter().map(|s| s.to_string()).collect());
    // 各模式字段组都构建好，连接 mode_row::notify-selected 切换可见性。
    let mode_row_clone = mode_row.clone();

    // —— 为每个模式构建一个 PreferencesGroup（含该模式字段）；不展示在当前模式的隐去 ——
    // 用 Rc<Vec<PreferencesGroup>> 备几模式切换引用。
    let groups: Vec<adw::PreferencesGroup> = Vec::new();
    let groups_handle = Rc::new(RefCell::new(groups));

    for &mode_key in mode_keys.iter() {
        let group = adw::PreferencesGroup::new();
        group.set_title(&format!("{} — {}",
            tr.get(lang, "folia.section.tuning"),
            tr.get(lang, &format!("folia.mode.{}", mode_key))));

        if mode_key == "cadenza" {
            // folia 上游未暴露 cadenza 面板，这里加个说明行
            let note = tr.get(lang, "folia.mode.cadenza.note");
            let r = adw::ActionRow::builder().title(note).subtitle("").build();
            group.add(&r);
        } else {
            let fields: Vec<super::folia_meta::Field> = super::folia_meta::FIELDS
                .iter().filter(|f| f.mode == mode_key).copied().collect();
            for field in fields {
                let title = if lang_en { field.en } else { field.zh };
                let row = build_folia_field_row(&field, &active, &state, title, lang);
                group.add(&row);
            }
        }

        group.set_visible(mode_key == mode_now);
        groups_handle.borrow_mut().push(group);
    }

    // —— 模式下拉切换：映射 selected → mode key，调出 folia_preset_set(visualizerMode) + 切换可见组 ——
    let gh_for_cb = Rc::clone(&groups_handle);
    mode_row.connect_selected_notify(move |r| {
        let idx = r.selected() as usize;
        // mode_keys_clone 来源 Rc; active_clone2; st_for_mode
        let key = mode_keys_clone.get(idx).map(|s| s.as_str()).unwrap_or("classic");
        folia_preset_set(&mut st_for_mode.borrow_mut().folia, &active_clone2, "visualizerMode", serde_json::json!(key));
        // 切换组可见性
        let groups = gh_for_cb.borrow();
        for (i, g) in groups.iter().enumerate() {
            g.set_visible(i == idx);
        }
    });

    page.add(&top_group);
    for g in groups_handle.borrow().iter() {
        page.add(g);
    }
    prefs.add(&page);
}

/// 单个 folia 字段 → 对应行控件：Bool/SwitchRow；Float/scale_row；Enum/ComboRow。
fn build_folia_field_row(field: &super::folia_meta::Field, active: &str, state: &State, title: &str, lang: Lang) -> gtk4::Widget {
    use super::folia_meta::{Kind, Opt};
    let lang_en = lang == Lang::En;
    let mode = field.mode;
    let path_owned = field.path.to_string();
    match field.kind {
        Kind::Bool | Kind::BoolOnOff | Kind::BoolShowHide | Kind::BoolOnOffZhSubtle => {
            // 布尔统一用 SwitchRow；三种标签变种仅影响 Combo而当选项列表文案不影响 SwitchRow。取当前默认标签文本作为行标题变量（已传）。
            // 实际只是个 toggle，'true' = shown/enable, 'false' = off/hide，默认值从现设定。
            let cur = {
                let s = state.borrow();
                folia_tuning_get(&s.folia, active, mode, &path_owned)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)              // 缺失回退 true （多数默认）
            };
            let row = adw::SwitchRow::builder().title(title).build();
            row.set_active(cur);
            let st = state.clone();
            let active_s = active.to_string();
            let path = path_owned.clone();
            row.connect_active_notify(move |r| {
                folia_tuning_set(&mut st.borrow_mut().folia, &active_s, mode, &path, serde_json::json!(r.is_active()));
            });
            row.upcast::<gtk4::Widget>()
        }
        Kind::Float { min, max, step } => {
            let cur = {
                let s = state.borrow();
                folia_tuning_get(&s.folia, active, mode, &path_owned)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0) as f32
            };
            let row = adw::ActionRow::builder().title(title).build();
            let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, min, max, step);
            scale.set_draw_value(true);
            scale.set_digits(if step >= 1.0 { 0 } else if step >= 0.05 { 2 } else { 3 });
            scale.set_hexpand(true);
            scale.set_width_request(260);
            scale.set_value(cur as f64);
            let st = state.clone();
            let active_s = active.to_string();
            let path = path_owned.clone();
            scale.connect_value_changed(move |s| {
                let v = s.value() as f32;
                folia_tuning_set(&mut st.borrow_mut().folia, &active_s, mode, &path, serde_json::json!(v));
            });
            row.add_suffix(&scale);
            row.upcast::<gtk4::Widget>()
        }
        Kind::Enum { opts } => {
            let cur_str = {
                let s = state.borrow();
                folia_tuning_get(&s.folia, active, mode, &path_owned)
                    .and_then(|v| v.as_str())
                    .unwrap_or("").to_string()
            };
            let labels: Vec<String> = opts.iter().map(|o: &Opt| {
                if lang_en { o.en.to_string() } else { o.zh.to_string() }
            }).collect();
            let labels_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let model = gtk4::StringList::new(&labels_refs);
            let combo = adw::ComboRow::builder().title(title).build();
            combo.set_model(Some(&model));
            let idx = opts.iter().position(|o| o.v == cur_str).unwrap_or(0) as u32;
            combo.set_selected(idx);
            let st = state.clone();
            let active_s = active.to_string();
            let path = path_owned.clone();
            // opts.v 生命周期为 'static，天将作闭共享
            let vals: Rc<Vec<&'static str>> = Rc::new(opts.iter().map(|o| o.v).collect());
            let vals_for_cb = vals.clone();
            combo.connect_selected_notify(move |r| {
                let i = r.selected() as usize;
                let v = vals_for_cb.get(i).copied().unwrap_or("");
                folia_tuning_set(&mut st.borrow_mut().folia, &active_s, mode, &path, serde_json::json!(v));
            });
            // 释放 vals 的所有权给闭包中不动 (引用计数) —— 弱 Rc 引用本身会在闭包中持有 vals.clone
            let _ = vals;
            combo.upcast::<gtk4::Widget>()
        }
    }
}

