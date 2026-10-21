mod actions;
mod config;
#[cfg(target_os = "macos")]
mod input_macos;
mod wheel;

/// 平台输入能力的统一入口（Windows 版后续补）。
mod input {
    #[cfg(target_os = "macos")]
    pub use crate::input_macos::{cursor_location, tap_active};

    #[cfg(not(target_os = "macos"))]
    pub fn cursor_location() -> Option<(f64, f64)> {
        None
    }
    #[cfg(not(target_os = "macos"))]
    pub fn tap_active() -> bool {
        false
    }
}

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| wheel::on_shortcut(app, shortcut, event.state))
                .build(),
        )
        .manage(wheel::WheelState::default())
        .manage(config::ConfigState::default())
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::set_config,
            config::reset_config,
            wheel::wheel_click
        ])
        .setup(|app| {
            let cfg = config::load(app.handle());
            *app.state::<config::ConfigState>().0.lock().unwrap() = cfg;

            // 不在 Dock 显示，作为菜单栏常驻工具
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            {
                let trusted = input_macos::ensure_accessibility(true);
                if input_macos::start_mouse_tap(app.handle().clone()) {
                    log::info!("启动：鼠标事件监听已开启");
                } else {
                    log::warn!(
                        "启动：鼠标事件监听开启失败（辅助功能已授权：{trusted}），退回轮询，跟手性会差一些"
                    );
                }
            }

            if let Err(e) = app.global_shortcut().register(wheel::toggle_shortcut()) {
                log::error!("启动：注册 ⌘⇧C 失败，可能被其他应用占用：{e}");
            }

            let open_item = MenuItem::with_id(app, "open", "打开菜单", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Ping Wheel（⌘⇧C）")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => actions::show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            log::info!("启动完成，按 ⌘⇧C 唤出轮盘");
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭主窗口时只隐藏，保持常驻
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
