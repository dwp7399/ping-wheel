//! 应用设置（目前只有模型配置），保存在应用配置目录的 settings.json。
//! API Key 只存在本机配置文件里（权限 600），不进仓库。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

/// 模型服务配置。目前对接火山方舟 Agent Plan（OpenAI 兼容），以后换成自建网关只需改这里。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSettings {
    /// OpenAI 兼容接口地址（不含 /chat/completions）
    pub base_url: String,
    pub api_key: String,
    /// 对话和看图用的模型（需支持图片输入）
    pub model: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            base_url: "https://ark.cn-beijing.volces.com/api/plan/v3".into(),
            api_key: String::new(),
            model: "glm-5.3-flash".into(),
        }
    }
}

impl LlmSettings {
    /// 配置里没填 Key 时，退回环境变量 PING_WHEEL_API_KEY（方便开发调试）。
    pub fn effective_key(&self) -> Option<String> {
        let key = self.api_key.trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
        std::env::var("PING_WHEEL_API_KEY").ok().filter(|k| !k.trim().is_empty())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub llm: LlmSettings,
}

#[derive(Default)]
pub struct SettingsState(pub Mutex<Settings>);

/// 给前端看的设置：Key 只返回是否已填写和末尾 4 位。
#[derive(Serialize)]
pub struct SettingsView {
    base_url: String,
    model: String,
    has_key: bool,
    key_hint: String,
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
    log::info!("设置：已保存 {}", p.display());
    Ok(())
}

pub fn llm(app: &AppHandle) -> LlmSettings {
    app.state::<SettingsState>().0.lock().unwrap().llm.clone()
}

fn view(s: &Settings) -> SettingsView {
    let key = s.llm.effective_key().unwrap_or_default();
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    SettingsView {
        base_url: s.llm.base_url.clone(),
        model: s.llm.model.clone(),
        has_key: !key.is_empty(),
        key_hint: if key.is_empty() { String::new() } else { format!("…{tail}") },
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> SettingsView {
    view(&state.0.lock().unwrap())
}

/// api_key 为 None 表示不修改已保存的 Key。
#[tauri::command]
pub fn settings_set_llm(
    app: AppHandle,
    state: State<'_, SettingsState>,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<SettingsView, String> {
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    if !base_url.starts_with("http") {
        return Err("接口地址需以 http(s):// 开头".into());
    }
    if model.trim().is_empty() {
        return Err("模型不能为空".into());
    }
    let mut s = state.0.lock().unwrap().clone();
    s.llm.base_url = base_url;
    s.llm.model = model.trim().to_string();
    if let Some(k) = api_key {
        s.llm.api_key = k.trim().to_string();
    }
    save(&app, &s)?;
    let v = view(&s);
    *state.0.lock().unwrap() = s;
    Ok(v)
}
