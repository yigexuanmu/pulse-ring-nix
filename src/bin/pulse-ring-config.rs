// pulse-ring-config — GTK4 + libadwaita 配置 GUI (独立进程)
// 负责读写 ~/.config/pulse-ring/pulse-ring.qml，中英双语。
//
// 拉入上游 Config 定义与 config_gui 模块。结构见 docs/settings-gui-design.md。

#[path = "../config.rs"]
mod config;

#[path = "../config_gui/mod.rs"]
mod config_gui;

use libadwaita as adw;
use gtk4::prelude::*;
use libadwaita::prelude::*;

fn main() {
    // config.rs uses log::warn!/info! — enable RUST_LOG if user wants verbose output.
    let _ = env_logger::try_init();

    let app = adw::Application::builder()
        .application_id("io.github.pulsering.Config")
        .build();
    app.connect_activate(|app| {
        config_gui::build_main_window(app);
    });
    app.run();
}
