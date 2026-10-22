//! 插件：以 zip 包安装 / 卸载，挂到轮盘的方向上运行。需要订阅「高级功能」。
//!
//! 包结构：
//!   manifest.json   插件描述（id、名称、版本、权限、动作）
//!   main.js         ES 模块，导出 run(ctx, api)
//!
//! 运行方式：每次运行开一个隐藏的窗口（标签 plugin-N），加载应用自带的运行器页面，
//! 运行器读取插件代码并执行。插件窗口只被授予 plugin_* 命令（见 capabilities/plugin.json），
//! 每个命令再按 manifest 里声明的权限检查。

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window};

use crate::scene::Scene;

pub const PERMISSIONS: &[(&str, &str)] = &[
    ("scene", "读取当前应用、窗口、链接和标记处的文字"),
    ("screenshot", "截取当前屏幕"),
    ("llm", "调用通用模型接口（消耗你的模型额度）"),
    ("clipboard", "写入剪贴板"),
    ("open", "打开网页链接"),
];

const RUN_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_ZIP: u64 = 5 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginAction {
    pub id: String,
    pub label: String,
    #[serde(default = "default_icon")]
    pub icon: String,
}

fn default_icon() -> String {
    "sparkle".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub actions: Vec<PluginAction>,
    #[serde(default = "default_main")]
    pub main: String,
}

fn default_main() -> String {
    "main.js".into()
}

impl Manifest {
    fn validate(&self) -> Result<(), String> {
        let id_ok = self.id.len() >= 2
            && self.id.len() <= 64
            && self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
            && self.id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
        if !id_ok {
            return Err(format!("插件 id「{}」无效：只能用小写字母、数字、点和横线", self.id));
        }
        if self.name.trim().is_empty() {
            return Err("插件名称不能为空".into());
        }
        if self.actions.is_empty() {
            return Err("插件至少要有一个动作".into());
        }
        let mut seen = std::collections::HashSet::new();
        for a in &self.actions {
            if a.id.trim().is_empty() || !seen.insert(a.id.as_str()) {
                return Err(format!("动作 id「{}」为空或重复", a.id));
            }
        }
        for p in &self.permissions {
            if !PERMISSIONS.iter().any(|(k, _)| k == p) {
                return Err(format!("未知权限「{p}」"));
            }
        }
        if self.main.contains("..") || self.main.starts_with('/') {
            return Err("main 路径无效".into());
        }
        Ok(())
    }

    fn has(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

fn plugins_dir(app: &AppHandle) -> PathBuf {
    let d = crate::store::data_dir(app).join("plugins");
    let _ = fs::create_dir_all(&d);
    d
}

fn data_file(app: &AppHandle, id: &str) -> PathBuf {
    let d = crate::store::data_dir(app).join("plugin-data");
    let _ = fs::create_dir_all(&d);
    d.join(format!("{id}.json"))
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(dir.join("manifest.json")).map_err(|_| "缺少 manifest.json".to_string())?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("manifest.json 格式错误：{e}"))?;
    m.validate()?;
    Ok(m)
}

pub fn get(app: &AppHandle, id: &str) -> Option<Manifest> {
    read_manifest(&plugins_dir(app).join(id)).ok()
}

#[derive(Serialize)]
pub struct PluginInfo {
    manifest: Manifest,
    permission_labels: Vec<String>,
}

#[tauri::command]
pub fn plugins_list(app: AppHandle) -> Vec<PluginInfo> {
    let mut out = vec![];
    if let Ok(entries) = fs::read_dir(plugins_dir(&app)) {
        for e in entries.flatten() {
            match read_manifest(&e.path()) {
                Ok(m) => {
                    let permission_labels = m
                        .permissions
                        .iter()
                        .filter_map(|p| PERMISSIONS.iter().find(|(k, _)| k == p).map(|(_, l)| l.to_string()))
                        .collect();
                    out.push(PluginInfo { manifest: m, permission_labels });
                }
                Err(err) => log::warn!("插件：{} 无效，已忽略：{err}", e.path().display()),
            }
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}

fn require_pro(app: &AppHandle) -> Result<(), String> {
    if crate::cloud::pro_active(app) {
        Ok(())
    } else {
        Err("自定义插件需要订阅「高级功能」".into())
    }
}

/// 从 zip 安装（同 id 覆盖升级）。
#[tauri::command]
pub async fn plugin_install(app: AppHandle, path: String) -> Result<Manifest, String> {
    if !crate::cloud::pro_active(&app) {
        let _ = crate::cloud::refresh_entitlements(&app).await;
    }
    require_pro(&app)?;
    let path = PathBuf::from(path);
    let size = fs::metadata(&path).map_err(|e| format!("读取插件包失败：{e}"))?.len();
    if size > MAX_ZIP {
        return Err("插件包超过 5MB".into());
    }
    let file = fs::File::open(&path).map_err(|e| format!("打开插件包失败：{e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("不是有效的 zip：{e}"))?;

    // 允许 zip 里多包一层目录
    let prefix = {
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        if names.iter().any(|n| n == "manifest.json") {
            String::new()
        } else {
            names
                .iter()
                .find(|n| n.ends_with("/manifest.json") && n.matches('/').count() == 1)
                .map(|n| n.trim_end_matches("manifest.json").to_string())
                .ok_or("zip 里没有 manifest.json")?
        }
    };

    let staging = plugins_dir(&app).join(format!(".staging-{}", crate::store::now_ms()));
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let result = (|| -> Result<Manifest, String> {
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = f.enclosed_name() else { continue }; // 拒绝 ../ 等越界路径
            let rel = rel.to_string_lossy().to_string();
            let Some(rel) = rel.strip_prefix(&prefix) else { continue };
            if rel.is_empty() || f.is_dir() {
                continue;
            }
            let dest = staging.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = vec![];
            f.by_ref().take(MAX_ZIP).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            fs::write(&dest, buf).map_err(|e| e.to_string())?;
        }
        let m = read_manifest(&staging)?;
        if !staging.join(&m.main).is_file() {
            return Err(format!("缺少入口文件 {}", m.main));
        }
        Ok(m)
    })();

    let m = match result {
        Ok(m) => m,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            log::warn!("插件：安装 {} 失败：{e}", path.display());
            return Err(e);
        }
    };
    let dest = plugins_dir(&app).join(&m.id);
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&staging, &dest).map_err(|e| format!("安装失败：{e}"))?;
    log::info!("插件：已安装 {} {}（{}）", m.name, m.version, m.id);
    let _ = app.emit("plugins:changed", &m.id);
    Ok(m)
}

/// 卸载：删除插件文件和数据，并从轮盘上移除它的动作。
#[tauri::command]
pub fn plugin_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    let dir = plugins_dir(&app).join(&id);
    if !dir.starts_with(plugins_dir(&app)) || id.contains('/') || id.contains("..") {
        return Err("插件 id 无效".into());
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("卸载失败：{e}"))?;
    let _ = fs::remove_file(data_file(&app, &id));
    crate::config::remove_plugin_items(&app, &id);
    log::info!("插件：已卸载 {id}");
    let _ = app.emit("plugins:changed", &id);
    Ok(())
}

// ---------- 运行 ----------

struct Run {
    manifest: Manifest,
    action: String,
    scene: Scene,
}

static RUNS: Mutex<Option<HashMap<String, Run>>> = Mutex::new(None);
static SEQ: AtomicU64 = AtomicU64::new(0);

fn with_runs<T>(f: impl FnOnce(&mut HashMap<String, Run>) -> T) -> T {
    let mut g = RUNS.lock().unwrap();
    f(g.get_or_insert_with(HashMap::new))
}

/// 轮盘选中插件动作时调用。
pub fn run(app: &AppHandle, plugin_id: &str, action_id: &str) -> Result<String, String> {
    require_pro(app)?;
    let manifest = get(app, plugin_id).ok_or_else(|| format!("插件 {plugin_id} 未安装"))?;
    let action = manifest
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(|| format!("插件没有动作 {action_id}"))?
        .label
        .clone();
    let scene = crate::scene::take();
    let label = format!("plugin-{}", SEQ.fetch_add(1, Ordering::Relaxed) + 1);
    with_runs(|m| m.insert(label.clone(), Run { manifest: manifest.clone(), action: action_id.to_string(), scene }));

    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("插件 · {}", manifest.name))
        .visible(false)
        .skip_taskbar(true)
        .build();
    if let Err(e) = built {
        with_runs(|m| m.remove(&label));
        return Err(format!("启动插件失败：{e}"));
    }
    log::info!("插件：运行 {}（{}）的「{action}」，窗口 {label}", manifest.name, manifest.id);

    // 超时保护
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(RUN_TIMEOUT);
        if with_runs(|m| m.remove(&label)).is_some() {
            log::warn!("插件：{label} 运行超时，已结束");
            if let Some(w) = app2.get_webview_window(&label) {
                let _ = w.close();
            }
        }
    });
    Ok(format!("正在运行「{}」", action))
}

/// 取当前窗口对应的运行（并校验调用者确实是插件窗口）。
fn run_of<T>(window: &Window, f: impl FnOnce(&Run) -> T) -> Result<T, String> {
    with_runs(|m| m.get(window.label()).map(f).ok_or_else(|| "插件运行已结束".to_string()))
}

fn need(window: &Window, perm: &str) -> Result<Manifest, String> {
    let m = run_of(window, |r| r.manifest.clone())?;
    if m.has(perm) {
        Ok(m)
    } else {
        Err(format!("插件没有声明「{perm}」权限"))
    }
}

/// 插件代码（运行器读取后执行）。
#[tauri::command]
pub fn plugin_source(app: AppHandle, window: Window) -> Result<String, String> {
    let m = run_of(&window, |r| r.manifest.clone())?;
    fs::read_to_string(plugins_dir(&app).join(&m.id).join(&m.main)).map_err(|e| format!("读取插件代码失败：{e}"))
}

/// 运行上下文：动作、插件信息；有 scene 权限时附带当前场景。
#[tauri::command]
pub fn plugin_ctx(window: Window) -> Result<Value, String> {
    run_of(&window, |r| {
        let mut ctx = json!({
            "action": r.action,
            "plugin": { "id": r.manifest.id, "name": r.manifest.name, "version": r.manifest.version },
        });
        if r.manifest.has("scene") {
            let s = &r.scene;
            ctx["scene"] = json!({
                "app": s.app_name,
                "title": s.window_title,
                "url": s.url,
                "file": s.doc_path,
                "selection": s.selection,
                "focusText": s.focus_text,
                "nearbyText": s.nearby_text,
                "pageText": s.page_text,
            });
        }
        ctx
    })
}

/// 截取当前屏幕，返回 JPEG data URL（标出了唤出点）。
#[tauri::command]
pub fn plugin_screenshot(app: AppHandle, window: Window) -> Result<String, String> {
    need(&window, "screenshot")?;
    let anchor = run_of(&window, |r| r.scene.anchor)?;
    if !crate::shot::ensure_screen_access() {
        return Err("需要屏幕录制权限".into());
    }
    let cap = crate::shot::capture_display_at(&crate::store::captures_dir(&app), anchor.0, anchor.1)?;
    let images = crate::shot::marked_images(&cap, anchor);
    let _ = fs::remove_file(&cap.path);
    Ok(images?.full)
}

/// 通用模型接口：body = { system?, messages: [{role, content}], max_tokens? }，返回正文。
#[tauri::command]
pub async fn plugin_llm(app: AppHandle, window: Window, body: Value) -> Result<String, String> {
    need(&window, "llm")?;
    crate::cloud::llm(&app, &body).await
}

#[tauri::command]
pub fn plugin_toast(app: AppHandle, window: Window, title: String, detail: String, status: Option<String>) -> Result<(), String> {
    let m = run_of(&window, |r| r.manifest.clone())?;
    let icon = m.actions.first().map(|a| a.icon.clone()).unwrap_or_else(default_icon);
    crate::actions::toast(&app, &icon, &title, &detail, status.as_deref().unwrap_or("done"));
    Ok(())
}

/// 在唤出点旁的悬浮面板里显示 Markdown 结果（可继续追问）。
#[tauri::command]
pub fn plugin_show(app: AppHandle, window: Window, title: String, markdown: String) -> Result<(), String> {
    run_of(&window, |_| ())?;
    crate::panel::show_result(&app, &title, &markdown)
}

#[tauri::command]
pub fn plugin_clipboard(app: AppHandle, window: Window, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    need(&window, "clipboard")?;
    app.clipboard().write_text(text).map_err(|e| format!("写入剪贴板失败：{e}"))
}

#[tauri::command]
pub fn plugin_open(app: AppHandle, window: Window, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    need(&window, "open")?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("只能打开 http(s) 链接".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// 插件自己的小型键值存储。
#[tauri::command]
pub fn plugin_storage_get(app: AppHandle, window: Window, key: String) -> Result<Value, String> {
    let id = run_of(&window, |r| r.manifest.id.clone())?;
    let data: Value = fs::read_to_string(data_file(&app, &id)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
    Ok(data.get(&key).cloned().unwrap_or(Value::Null))
}

#[tauri::command]
pub fn plugin_storage_set(app: AppHandle, window: Window, key: String, value: Value) -> Result<(), String> {
    let id = run_of(&window, |r| r.manifest.id.clone())?;
    let path = data_file(&app, &id);
    let mut data: Value = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
    data[key] = value;
    let text = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    if text.len() > 1024 * 1024 {
        return Err("插件存储超过 1MB".into());
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

/// 插件运行结束（error 为 None 表示成功）。
#[tauri::command]
pub fn plugin_finish(app: AppHandle, window: Window, error: Option<String>) {
    let label = window.label().to_string();
    let run = with_runs(|m| m.remove(&label));
    if let (Some(r), Some(e)) = (&run, &error) {
        log::warn!("插件：{} 运行出错：{e}", r.manifest.id);
        let icon = r.manifest.actions.first().map(|a| a.icon.clone()).unwrap_or_else(default_icon);
        crate::actions::toast(&app, &icon, &r.manifest.name, e, "failed");
    }
    let _ = window.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, perms: &[&str]) -> Manifest {
        Manifest {
            id: id.into(),
            name: "测试".into(),
            version: "1.0.0".into(),
            description: String::new(),
            author: String::new(),
            permissions: perms.iter().map(|s| s.to_string()).collect(),
            actions: vec![PluginAction { id: "a".into(), label: "A".into(), icon: "sparkle".into() }],
            main: "main.js".into(),
        }
    }

    #[test]
    fn manifest_validation() {
        assert!(manifest("com.example.tr", &["llm", "scene"]).validate().is_ok());
        assert!(manifest("Bad_ID", &[]).validate().is_err());
        assert!(manifest("../evil", &[]).validate().is_err());
        assert!(manifest("ok.id", &["root"]).validate().is_err());
        let mut m = manifest("ok.id", &[]);
        m.main = "../x.js".into();
        assert!(m.validate().is_err());
    }
}
