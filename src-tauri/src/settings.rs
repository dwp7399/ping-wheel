//! 应用设置：账号（后端服务地址 + 用户令牌），保存在应用配置目录的 settings.json（权限 600）。
//! 客户端不直接调用任何模型服务，模型能力全部经由后端（见 cloud.rs）。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

pub const DEFAULT_SERVER: &str = "http://127.0.0.1:8787";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    pub server_url: String,
    pub token: String,
}

impl Default for Account {
    fn default() -> Self {
        Self { server_url: DEFAULT_SERVER.into(), token: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub account: Account,
    /// 语言："auto"（按系统时区）或支持的语言代码
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { account: Account::default(), language: "auto".into() }
    }
}

#[derive(Default)]
pub struct SettingsState(pub Mutex<Settings>);

/// 给前端看的账号信息：令牌只返回是否已填写和末尾 4 位。
#[derive(Serialize)]
pub struct AccountView {
    server_url: String,
    has_token: bool,
    token_hint: String,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    let Some(p) = path(app) else {
        return Settings::default();
    };
    match fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            log::warn!("设置：解析 {} 失败：{e}，使用默认设置", p.display());
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let p = path(app).ok_or("无法确定配置目录")?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("写入 {} 失败：{e}", p.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn account(app: &AppHandle) -> Account {
    app.state::<SettingsState>().0.lock().unwrap().account.clone()
}

fn view(a: &Account) -> AccountView {
    let tail: String = a.token.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    AccountView {
        server_url: a.server_url.clone(),
        has_token: !a.token.is_empty(),
        token_hint: if a.token.is_empty() { String::new() } else { format!("…{tail}") },
    }
}

#[tauri::command]
pub fn account_get(state: State<'_, SettingsState>) -> AccountView {
    view(&state.0.lock().unwrap().account)
}

pub fn set_token(app: &AppHandle, token: &str) -> Result<(), String> {
    let state = app.state::<SettingsState>();
    let mut s = state.0.lock().unwrap().clone();
    s.account.token = token.to_string();
    save(app, &s)?;
    *state.0.lock().unwrap() = s;
    Ok(())
}

/// 退出登录：通知服务端作废令牌，再清掉本地令牌。
#[tauri::command]
pub async fn account_logout(app: AppHandle) -> Result<(), String> {
    crate::cloud::logout(&app).await;
    set_token(&app, "")?;
    *app.state::<crate::cloud::CloudState>().0.lock().unwrap() = None;
    log::info!("账号：已退出登录");
    Ok(())
}

#[derive(Serialize)]
pub struct LocaleView {
    /// 设置值："auto" 或语言代码
    setting: String,
    /// 实际使用的语言
    effective: String,
    timezone: String,
    langs: Vec<(String, String)>,
}

fn locale_view(setting: &str) -> LocaleView {
    LocaleView {
        setting: setting.to_string(),
        effective: crate::i18n::current(),
        timezone: crate::i18n::detect_timezone().unwrap_or_default(),
        langs: crate::i18n::LANGS.iter().map(|(c, n)| (c.to_string(), n.to_string())).collect(),
    }
}

#[tauri::command]
pub fn locale_get(state: State<'_, SettingsState>) -> LocaleView {
    locale_view(&state.0.lock().unwrap().language)
}

#[tauri::command]
pub fn locale_set(app: AppHandle, language: String) -> Result<LocaleView, String> {
    use tauri::Emitter;
    let language = if crate::i18n::is_supported(&language) { language } else { "auto".into() };
    let state = app.state::<SettingsState>();
    let mut s = state.0.lock().unwrap().clone();
    s.language = language.clone();
    save(&app, &s)?;
    *state.0.lock().unwrap() = s;
    let lang = crate::i18n::apply(&language);
    log::info!("设置：语言改为 {lang}（设置 {language}）");
    let _ = app.emit("locale:changed", &lang);
    Ok(locale_view(&language))
}

/// token 为 None 表示不修改已保存的令牌。
#[tauri::command]
pub async fn account_set(app: AppHandle, server_url: String, token: Option<String>) -> Result<AccountView, String> {
    let server_url = server_url.trim().trim_end_matches('/').to_string();
    if !server_url.starts_with("http") {
        return Err(t!("服务地址需以 http(s):// 开头"));
    }
    let state = app.state::<SettingsState>();
    let mut s = state.0.lock().unwrap().clone();
    s.account.server_url = server_url;
    if let Some(t) = token {
        s.account.token = t.trim().to_string();
    }
    save(&app, &s)?;
    let v = view(&s.account);
    *state.0.lock().unwrap() = s;
    crate::cloud::refresh_entitlements(&app).await.ok();
    Ok(v)
}
