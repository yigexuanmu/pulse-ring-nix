// pulse-ring-config — GTK4 + libadwaita 配置 GUI (独立进程)
// 负责读写 ~/.config/pulse-ring/pulse-ring.qml + folia-lyrics.json，双语。
//
// Phase 1 骨架：最小 libadwaita 窗口，验证 GTK4 构建链通。后续按
// docs/settings-gui-design.md 逐步填充各 PreferencePage。

use gtk4::prelude::*;
use libadwaita::prelude::*;
use libadwaita as adw;

fn main() {
    let app = adw::Application::builder()
        .application_id("io.github.pulsering.Config")
        .build();
    app.connect_activate(|app| build_ui(app));
    app.run();
}

fn build_ui(app: &adw::Application) {
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("pulse-ring 配置 / Settings")
        .default_width(900)
        .default_height(640)
        .build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let status = gtk4::Label::new(Some("GUI skeleton — GTK4 build chain OK"));
    status.set_margin_top(40);
    status.set_margin_bottom(40);

    let content = adw::StatusPage::builder()
        .title("pulse-ring 配置")
        .description("GUI 骨架加载成功。各配置页待填充。 / Skeleton loaded; pages pending.")
        .child(&status)
        .build();

    toolbar_view.set_content(Some(&content));
    win.set_content(Some(&toolbar_view));
    win.present();
}
