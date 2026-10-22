// 让应用自带命令也走权限控制：主界面窗口拿全部，插件窗口只拿 plugin_* 命令
// （见 capabilities/default.json 与 capabilities/plugin.json）。
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_config",
            "set_config",
            "reset_config",
            "wheel_click",
            "account_get",
            "account_set",
            "account_me",
            "image_data_url",
            "chat_open",
            "marks_list",
            "mark_restore",
            "mark_delete",
            "mark_note",
            "collections_list",
            "collection_delete",
            "collection_reclassify",
            "collection_open_source",
            "panel_state",
            "panel_send",
            "panel_pin",
            "panel_close",
            "plugins_list",
            "plugin_install",
            "plugin_uninstall",
            "plugin_source",
            "plugin_ctx",
            "plugin_screenshot",
            "plugin_llm",
            "plugin_toast",
            "plugin_show",
            "plugin_clipboard",
            "plugin_open",
            "plugin_storage_get",
            "plugin_storage_set",
            "plugin_finish",
        ]),
    ))
    .expect("failed to run tauri-build");
}
