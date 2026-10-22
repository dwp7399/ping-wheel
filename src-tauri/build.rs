// 让应用自带命令也走权限控制：主界面窗口拿全部，插件窗口只拿 plugin_* 命令
// （见 capabilities/default.json 与 capabilities/plugin.json）。

/// 构建期服务根地址。空字符串表示不注入，运行时用本机开发地址。
/// 发布 job 在工作流里拒绝空值；这里不看 CI，避免分支上的检查因未设置变量失败。
struct ServerUrlEnv;

impl ServerUrlEnv {
    fn resolve() -> String {
        println!("cargo:rerun-if-env-changed=PING_WHEEL_SERVER_URL");
        let url = std::env::var("PING_WHEEL_SERVER_URL").unwrap_or_default();
        let url = url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            return String::new();
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            panic!("PING_WHEEL_SERVER_URL 需以 http:// 或 https:// 开头，当前为 {url}");
        }
        url
    }
}

fn main() {
    let url = ServerUrlEnv::resolve();
    println!("cargo:rustc-env=PING_WHEEL_SERVER_URL={url}");

    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_config",
            "set_config",
            "reset_config",
            "wheel_click",
            "account_get",
            "account_set",
            "account_logout",
            "locale_get",
            "locale_set",
            "login_email_start",
            "login_email_verify",
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
