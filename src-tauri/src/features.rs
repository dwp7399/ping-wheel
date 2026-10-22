//! 核心功能：问号（截图 + 场景理解）、对话、标记（记录并回到场景）、收藏（截图 + 自动归类）。
//! 模型能力一律经由 crate::cloud 调用后端；问号和对话的结果显示在悬浮面板（panel.rs）里，
//! 面板关闭即结束，不保存对话历史。

use std::path::Path;
use std::process::Command;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::cloud;
use crate::panel::{self, Kind};
use crate::scene;
use crate::shot;
use crate::store;

/// 等轮盘窗口消失后截取光标所在屏幕。
fn screenshot(app: &AppHandle) -> Result<shot::Capture, String> {
    if !shot::ensure_screen_access() {
        return Err(t!("需要屏幕录制权限：请在「系统设置 → 隐私与安全性 → 屏幕录制」里允许 Ping Wheel，然后重启它"));
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
    let (x, y) = crate::wheel::anchor(app);
    let cap = shot::capture_display_at(&store::captures_dir(app), x, y)?;
    log::info!("截图：已保存 {}", cap.path.display());
    Ok(cap)
}

/// 只允许读取截图目录里的文件（标记 / 收藏的缩略图）。
#[tauri::command]
pub fn image_data_url(app: AppHandle, path: String) -> Result<String, String> {
    use base64::Engine;
    let dir = store::captures_dir(&app).canonicalize().map_err(|e| e.to_string())?;
    let p = Path::new(&path).canonicalize().map_err(|_| t!("图片不存在"))?;
    if !p.starts_with(&dir) {
        return Err(t!("不允许读取该路径"));
    }
    let bytes = std::fs::read(&p).map_err(|e| t!("读取图片失败：{e}", e = e))?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n { format!("{t}…") } else { t }
}

// ---------- 问号 ----------

/// 问号：全屏截图（标出问号位置）+ 放大图 + 场景文字 → 后端理解，结果在悬浮面板里流式显示。
/// 截图只发给模型，不展示给用户；截图文件用完即删。
pub fn ask(app: &AppHandle, question: &str, mode: &str, custom: &str) -> Result<String, String> {
    let scene = scene::take();
    let cap = screenshot(app)?;
    let images = shot::marked_images(&cap, scene.anchor);
    let _ = std::fs::remove_file(&cap.path);
    let images = images?;

    let body = json!({
        "question": question.trim(),
        "mode": mode,
        "custom_prompt": custom.trim(),
        "context": scene.context_json(Some(images.marker)),
        "images": { "full": images.full, "crop": images.crop },
    });
    let title = scene.headline();
    panel::open(app, Kind::Ask { body }, &title, true)?;
    Ok(t!("正在理解「{title}」", title = short(&title, 18)))
}

// ---------- 对话 ----------

pub fn chat(app: &AppHandle) -> Result<String, String> {
    panel::open(app, Kind::Chat, "", false)?;
    Ok(t!("已打开对话"))
}

#[tauri::command]
pub fn chat_open(app: AppHandle) -> Result<(), String> {
    panel::open(&app, Kind::Chat, "", false)
}

// ---------- 标记 ----------

/// 标记：记录当前场景 + 截图缩略图。
pub fn mark(app: &AppHandle) -> Result<String, String> {
    let scene = scene::take();
    let image = match screenshot(app) {
        Ok(c) => Some(c.path.to_string_lossy().into_owned()),
        Err(e) => {
            log::warn!("标记：截图失败，只保存场景信息：{e}");
            None
        }
    };
    let id = store::add_mark(app, &scene, image.as_deref())?;
    let _ = app.emit("marks:changed", id);
    let how = if scene.url.is_some() {
        t!("可回到这个网页")
    } else if scene.doc_path.is_some() {
        t!("可回到这个文件")
    } else {
        t!("可回到这个应用")
    };
    log::info!("标记：已保存 {id}「{}」", scene.headline());
    Ok(format!("{}（{how}）", short(&scene.headline(), 18)))
}

/// 用指定应用打开目标；应用没运行时 open 会自动启动它。
/// 应用路径失效（被移动 / 重装）时按 bundle id 找。
fn open_with(app_path: Option<&str>, bundle_id: Option<&str>, target: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("open");
    if let Some(a) = app_path.filter(|a| Path::new(a).exists()) {
        cmd.arg("-a").arg(a);
    } else if let Some(b) = bundle_id.filter(|b| !b.is_empty()) {
        cmd.arg("-b").arg(b);
    } else if target.is_none() {
        return Err(t!("找不到对应的应用"));
    }
    if let Some(t) = target {
        cmd.arg(t);
    }
    let out = cmd.output().map_err(|e| t!("调用 open 失败：{e}", e = e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[tauri::command]
pub fn marks_list(app: AppHandle) -> Result<Vec<store::MarkRow>, String> {
    store::list_marks(&app)
}

#[tauri::command]
pub fn mark_restore(app: AppHandle, id: i64) -> Result<String, String> {
    let m = store::get_mark(&app, id)?.ok_or_else(|| t!("标记不存在"))?;
    let target = m.url.clone().or_else(|| m.doc_path.clone().filter(|p| Path::new(p).exists()));
    let result = open_with(m.app_path.as_deref(), m.bundle_id.as_deref(), target.as_deref());
    match &result {
        Ok(()) => log::info!("标记：已回到 {id}（{}）", target.as_deref().unwrap_or(&m.app_name)),
        Err(e) => log::warn!("标记：回到 {id} 失败：{e}"),
    }
    result.map(|_| match (&m.url, &m.doc_path) {
        (Some(u), _) => t!("已打开 {target}", target = u),
        (_, Some(p)) => t!("已打开 {target}", target = p),
        _ => t!("已切换到 {app}", app = m.app_name),
    })
}

#[tauri::command]
pub fn mark_delete(app: AppHandle, id: i64) -> Result<(), String> {
    if let Some(img) = store::delete_mark(&app, id)? {
        let _ = std::fs::remove_file(img);
    }
    Ok(())
}

#[tauri::command]
pub fn mark_note(app: AppHandle, id: i64, note: String) -> Result<(), String> {
    store::set_mark_note(&app, id, note.trim())
}

// ---------- 收藏 ----------

/// 收藏：截图 + 场景先入库，再在后台由后端归类。
pub fn collect(app: &AppHandle) -> Result<String, String> {
    let scene = scene::take();
    let cap = screenshot(app)?;
    let key = scene.source_key();
    let id = store::add_collection(app, &scene, &cap.path.to_string_lossy(), &key)?;
    let _ = app.emit("collections:changed", id);
    log::info!("收藏：已保存 {id}，来源 {key}");
    let app2 = app.clone();
    let context = scene.context_json(None);
    tauri::async_runtime::spawn(async move { classify(app2, id, Some(context)).await });
    Ok(t!("已收藏「{title}」，正在自动归类", title = short(&scene.headline(), 18)))
}

async fn classify(app: AppHandle, id: i64, context: Option<serde_json::Value>) {
    let Ok(Some(row)) = store::get_collection(&app, id) else { return };
    let _ = store::set_collection_status(&app, id, "classifying");
    let _ = app.emit("collections:changed", id);

    let image = match shot::data_url(Path::new(&row.image_path), 1600) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("收藏：读取截图失败（{id}）：{e}");
            let _ = store::set_collection_status(&app, id, "failed");
            let _ = app.emit("collections:changed", id);
            return;
        }
    };
    let context = context.unwrap_or_else(|| {
        json!({ "app": row.app_name, "title": row.window_title, "url": row.url })
    });
    let body = json!({
        "context": context,
        "image": image,
        "categories": store::categories(&app).unwrap_or_default(),
        "same_source_category": store::category_of_source(&app, &row.source_key, id).ok().flatten(),
    });
    match cloud::classify(&app, &body).await {
        Ok(v) => {
            let s = |k: &str| v[k].as_str().unwrap_or("").trim().to_string();
            let tags: Vec<String> = v["tags"]
                .as_array()
                .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let title = if s("title").is_empty() { row.title.clone() } else { s("title") };
            let _ = store::classify_collection(&app, id, &title, &s("category"), &tags, &s("summary"), "done");
            log::info!("收藏：{id} 已归类为「{}」", s("category"));
        }
        Err(e) => {
            log::warn!("收藏：{id} 归类失败：{e}");
            let _ = store::set_collection_status(&app, id, "failed");
        }
    }
    let _ = app.emit("collections:changed", id);
}

#[tauri::command]
pub fn collections_list(app: AppHandle) -> Result<Vec<store::CollectionRow>, String> {
    store::list_collections(&app)
}

#[tauri::command]
pub fn collection_delete(app: AppHandle, id: i64) -> Result<(), String> {
    if let Some(img) = store::delete_collection(&app, id)? {
        let _ = std::fs::remove_file(img);
    }
    Ok(())
}

#[tauri::command]
pub fn collection_reclassify(app: AppHandle, id: i64) {
    tauri::async_runtime::spawn(async move { classify(app, id, None).await });
}

#[tauri::command]
pub fn collection_open_source(app: AppHandle, id: i64) -> Result<(), String> {
    let c = store::get_collection(&app, id)?.ok_or_else(|| t!("收藏不存在"))?;
    let target = c.url.clone().or_else(|| c.doc_path.clone());
    open_with(c.app_path.as_deref(), None, target.as_deref())
}
