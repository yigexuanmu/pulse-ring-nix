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

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(900)
        .default_height(680)
        .build();

    // —— HeaderBar ——
    let header = adw::HeaderBar::new();

    // 语言切换按钮：写偏好 + 弹提示 (重开 GUI 生效)
    let lang_btn = gtk4::Button::with_label(tr.get(lang, "tab.language"));
    lang_btn.add_css_class("flat");
    let st = state.clone();
    lang_btn.connect_clicked(move |_| {
        let cur = st.borrow().lang;
        let next = cur.other();
        st.borrow_mut().lang = next;
        save_lang_pref(next);
        let msg = format!(
            "{} → {} ({})",
            tr.get(cur, "tab.language"),
            next.code(),
            tr.get(cur, "lang.note"),
        );
        info_dialog(&msg);
    });
    header.pack_start(&lang_btn);

    // 保存按钮：写盘 + 弹提示
    let save_btn = gtk4::Button::with_label(tr.get(lang, "common.save"));
    save_btn.add_css_class("suggested-action");
    let st = state.clone();
    save_btn.connect_clicked(move |_| {
        let s = st.borrow();
        match qml_io::save_config(&s.config) {
            Ok(_) => info_dialog(&s.tr.get(s.lang, "common.savedHint")),
            Err(e) => {
                info_dialog(&format!("{}: {}", s.tr.get(s.lang, "common.save"), e))
            }
        }
    });
    header.pack_end(&save_btn);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    // —— 多 PreferencesPage ——
    let prefs = adw::PreferencesWindow::new();
    prefs.set_search_enabled(true);

    build_shape_color_page(&prefs, state.clone(), &tr, lang);
    build_rings_page(&prefs, state.clone(), &tr, lang);
    build_spawn_page(&prefs, state.clone(), &tr, lang);
    build_audio_page(&prefs, state.clone(), &tr, lang);
    build_language_page(&prefs, state.clone(), &tr, lang);
    build_stub_pages(&prefs, &tr, lang);

    toolbar.set_content(Some(&prefs));
    win.set_content(Some(&toolbar));
    win.set_title(Some(&title));
    win.present();
}

// —— helpers ——

/// 弹一个非阻塞信息对话框（v1 用 gtk4 MessageDialog）。
fn info_dialog(msg: &str) {
    use gtk4::Dialog;
    use gtk4::MessageType;
    let dlg = gtk4::MessageDialog::builder()
        .text(msg)
        .message_type(MessageType::Info)
        .buttons(gtk4::ButtonsType::Ok)
        .build();
    dlg.connect_response(|d, _| d.close());
    dlg.present();
}

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
    let stubs: [(&str, &str, &str); 4] = [
        (tr.get(lang, "tab.particles"), "preferences-desktop-effects-symbolic", "（v2 填充）"),
        (tr.get(lang, "tab.wallpaper"), "preferences-desktop-wallpaper-symbolic", "（v2 填充）"),
        (tr.get(lang, "tab.widgets"), "view-grid-symbolic", "（v2 填充）"),
        (tr.get(lang, "tab.lyric"), "applications-multimedia-symbolic", "（v2 填充：folia 11 模式 Tuning）"),
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
