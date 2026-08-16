// config_gui — 独立配置 GUI（bin: pulse-ring-config）。
//
// 模块结构：
//   - app      : 主窗口构建 + 各标签页（libadwaita）。
//   - i18n     : Lang + Tr，双语字符串表。
//   - qml_io   : Config 加载/保存（QML 序列化器，round-trip 上游 parser）。
//
// bin 通过 `#[path = "../config_gui/mod.rs"] mod config_gui;` 把本模块拉入 bin crate，
// 同时 `#[path = "../config.rs"] mod config;` 把上游 Config 类型也拉入 bin。
// 这样 GUI 与上游 pulse-ring 二进制共用同一份 Config 定义，不重复维护。

pub mod app;
pub mod folia_meta;
pub mod i18n;
pub mod qml_io;

pub use app::build_main_window;
