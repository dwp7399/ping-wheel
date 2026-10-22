//! 四个核心功能：问号（截图问 AI）、对话、标记（记录并回到场景）、收藏（截图 + 自动归类）。
//! 模型调用一律走 crate::llm。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager};

use crate::llm::{self, Delta, Message, Part, Purpose, Request, Role};
use crate::scene::{self, Scene};
use crate::store;

const DEFAULT_ASK: &str = "请解释截图里我正在看的内容：它是什么、关键点是什么、有什么需要注意的。";

fn system_prompt(scene: Option<&Scene>) -> String {
    let mut s = String::from(
        "你是 Ping Wheel 桌面助手。用户通过快捷键随时向你提问，常会附上当前屏幕截图。\
         请用简体中文回答，先给结论再展开，简洁、具体，必要时用 Markdown 列表或代码块。",
    );
    if let Some(sc) = scene {
        s.push_str("\n\n用户提问时的场景：\n");
        s.push_str(&sc.describe());
    }
    s
}

// ---------- 截图 ----------

/// 等轮盘窗口消失后截取光标所在屏幕。
fn screenshot(app: &AppHandle) -> Result<PathBuf, String> {
    if !scene::ensure_screen_access() {
        return Err("需要屏幕录制权限：请在「系统设置 → 隐私与安全性 → 屏幕录制」里允许 Ping Wheel，然后重启它".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
    let (x, y) = crate::wheel::anchor(app);
    scene::capture_display_at(&store::captures_dir(app), x, y)
}

/// 只允许读取截图目录里的文件。
fn checked_capture_path(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let dir = store::captures_dir(app).canonicalize().map_err(|e| e.to_string())?;
    let p = Path::new(path).canonicalize().map_err(|_| "图片不存在".to_string())?;
    if !p.starts_with(&dir) {
        return Err("不允许读取该路径".into());
    }
    Ok(p)
}

#[tauri::command]
pub fn image_data_url(app: AppHandle, path: String) -> Result<String, String> {
    use base64::Engine;
    let p = checked_capture_path(&app, &path)?;
    let bytes = std::fs::read(&p).map_err(|e| format!("读取图片失败：{e}"))?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

// ---------- 对话窗口 ----------

const CHAT_H: f64 = 640.0;

fn show_chat(app: &AppHandle, conversation_id: i64, near_anchor: bool) {
    let Some(win) = app.get_webview_window("chat") else {
        log::error!("对话：找不到 chat 窗口");
        return;
    };
    if near_anchor {
        // 放在唤出点右侧，靠近用户视线；超出屏幕时由系统约束
        let (x, y) = crate::wheel::anchor(app);
        let _ = win.set_position(LogicalPosition::new(x + 24.0, (y - CHAT_H / 2.0).max(40.0)));
    }
    let _ = win.emit("chat:open", conversation_id);
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
}

#[derive(Clone, Serialize)]
struct DeltaPayload<'a> {
    conversation_id: i64,
    message_id: i64,
    content: Option<&'a str>,
    reasoning: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct DonePayload {
    conversation_id: i64,
    message_id: i64,
    status: String,
    content: String,
}

/// 把对话历史整理成模型消息（带图的消息重新编码图片）。
fn history_messages(app: &AppHandle, conversation_id: i64, scene: Option<&Scene>) -> Result<Vec<Message>, String> {
    let mut out = vec![Message::text(Role::System, system_prompt(scene))];
    for m in store::list_messages(app, conversation_id)? {
        if m.status == "error" || (m.role == "assistant" && m.content.is_empty()) {
            continue;
        }
        let role = if m.role == "user" { Role::User } else { Role::Assistant };
        let mut parts = vec![Part::Text(m.content.clone())];
        if let Some(img) = &m.image_path {
            match scene::image_data_url_for_llm(Path::new(img)) {
                Ok(url) => parts.push(Part::Image(url)),
                Err(e) => log::warn!("对话：附图 {img} 读取失败：{e}"),
            }
        }
        out.push(Message { role, parts });
    }
    Ok(out)
}

/// 让模型回复对话的最新一条消息（流式推送到对话窗口）。
async fn respond(app: AppHandle, conversation_id: i64, purpose: Purpose, scene: Option<Scene>) {
    let messages = match history_messages(&app, conversation_id, scene.as_ref()) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("对话：整理历史失败（对话 {conversation_id}）：{e}");
            return;
        }
    };
    let message_id = match store::add_message(&app, conversation_id, "assistant", "", None, "streaming") {
        Ok(id) => id,
        Err(e) => {
            log::warn!("对话：创建回复失败（对话 {conversation_id}）：{e}");
            return;
        }
    };
    let _ = app.emit_to("chat", "chat:changed", conversation_id);

    let result = llm::stream(&app, Request { purpose, messages, max_tokens: Some(4096) }, |d| {
        let (content, reasoning) = match d {
            Delta::Content(c) => (Some(c), None),
            Delta::Reasoning(r) => (None, Some(r)),
        };
        let _ = app.emit_to("chat", "chat:delta", DeltaPayload { conversation_id, message_id, content, reasoning });
    })
    .await;

    let (status, content, reasoning) = match result {
        Ok(r) => ("done", r.content, r.reasoning),
        Err(e) => ("error", e, String::new()),
    };
    let _ = store::finish_message(&app, message_id, &content, &reasoning, status);
    let _ = app.emit_to(
        "chat",
        "chat:done",
        DonePayload { conversation_id, message_id, status: status.into(), content },
    );
}

/// 问号：截图 + 场景 → 新对话 → 打开对话窗口并开始回答。
pub fn ask(app: &AppHandle, prompt: &str) -> Result<String, String> {
    let scene = scene::take();
    let image = screenshot(app)?;
    let question = if prompt.trim().is_empty() { DEFAULT_ASK } else { prompt.trim() };
    let conv = store::create_conversation(app, &format!("问号 · {}", scene.headline()))?;
    store::add_message(app, conv, "user", question, Some(&image.to_string_lossy()), "done")?;
    show_chat(app, conv, true);
    log::info!("问号：新对话 {conv}，场景「{}」", scene.headline());
    let app2 = app.clone();
    tauri::async_runtime::spawn(respond(app2, conv, Purpose::Ask, Some(scene)));
    Ok(format!("已截图，正在解释「{}」", scene_short(&question)))
}

fn scene_short(s: &str) -> String {
    let t: String = s.chars().take(18).collect();
    if s.chars().count() > 18 { format!("{t}…") } else { t }
}

/// 对话：打开对话窗口并新开一个对话。
pub fn chat(app: &AppHandle) -> Result<String, String> {
    let conv = store::create_conversation(app, "")?;
    show_chat(app, conv, true);
    Ok("已打开新对话".into())
}

#[tauri::command]
pub fn chat_new(app: AppHandle) -> Result<i64, String> {
    store::create_conversation(&app, "")
}

#[tauri::command]
pub fn chat_list(app: AppHandle) -> Result<Vec<store::ConversationRow>, String> {
    store::list_conversations(&app)
}

#[tauri::command]
pub fn chat_messages(app: AppHandle, conversation_id: i64) -> Result<Vec<store::MessageRow>, String> {
    store::list_messages(&app, conversation_id)
}

#[tauri::command]
pub fn chat_delete(app: AppHandle, conversation_id: i64) -> Result<(), String> {
    store::delete_conversation(&app, conversation_id)
}

#[tauri::command]
pub async fn chat_send(app: AppHandle, conversation_id: i64, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("内容为空".into());
    }
    store::add_message(&app, conversation_id, "user", &text, None, "done")?;
    if store::conversation_title(&app, conversation_id)?.unwrap_or_default().is_empty() {
        store::set_conversation_title(&app, conversation_id, &scene_short(&text))?;
    }
    tauri::async_runtime::spawn(respond(app, conversation_id, Purpose::Chat, None));
    Ok(())
}

#[tauri::command]
pub fn chat_open_window(app: AppHandle, conversation_id: Option<i64>) -> Result<(), String> {
    let id = match conversation_id {
        Some(id) => id,
        None => store::create_conversation(&app, "")?,
    };
    show_chat(&app, id, false);
    Ok(())
}

// ---------- 标记 ----------

/// 标记：记录当前场景 + 截图缩略图。
pub fn mark(app: &AppHandle) -> Result<String, String> {
    let scene = scene::take();
    let image = match screenshot(app) {
        Ok(p) => Some(p.to_string_lossy().into_owned()),
        Err(e) => {
            log::warn!("标记：截图失败，只保存场景信息：{e}");
            None
        }
    };
    let id = store::add_mark(app, &scene, image.as_deref())?;
    let _ = app.emit("marks:changed", id);
    let how = if scene.url.is_some() {
        "可回到这个网页"
    } else if scene.doc_path.is_some() {
        "可回到这个文件"
    } else {
        "可回到这个应用"
    };
    log::info!("标记：已保存 {id}「{}」", scene.headline());
    Ok(format!("{}（{how}）", scene_short(&scene.headline())))
}

/// 在指定应用里打开目标；没有应用信息时交给系统默认方式。
fn open_with(app_path: Option<&str>, target: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("open");
    if let Some(a) = app_path.filter(|a| Path::new(a).exists()) {
        cmd.arg("-a").arg(a);
    } else if target.is_none() {
        return Err("找不到对应的应用".into());
    }
    if let Some(t) = target {
        cmd.arg(t);
    }
    let out = cmd.output().map_err(|e| format!("调用 open 失败：{e}"))?;
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
    let m = store::get_mark(&app, id)?.ok_or("标记不存在")?;
    let target = m.url.clone().or_else(|| m.doc_path.clone().filter(|p| Path::new(p).exists()));
    let result = open_with(m.app_path.as_deref(), target.as_deref());
    match &result {
        Ok(()) => log::info!("标记：已回到 {id}（{}）", target.as_deref().unwrap_or(&m.app_name)),
        Err(e) => log::warn!("标记：回到 {id} 失败：{e}"),
    }
    result.map(|_| match (&m.url, &m.doc_path) {
        (Some(u), _) => format!("已打开 {u}"),
        (_, Some(p)) => format!("已打开 {p}"),
        _ => format!("已切换到 {}", m.app_name),
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

/// 收藏：截图 + 场景先入库，再在后台让模型归类。
pub fn collect(app: &AppHandle) -> Result<String, String> {
    let scene = scene::take();
    let image = screenshot(app)?;
    let key = scene.source_key();
    let id = store::add_collection(app, &scene, &image.to_string_lossy(), &key)?;
    let _ = app.emit("collections:changed", id);
    log::info!("收藏：已保存 {id}，来源 {key}");
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { classify(app2, id).await });
    Ok(format!("已收藏「{}」，正在自动归类", scene_short(&scene.headline())))
}

#[derive(Debug, PartialEq)]
struct Classified {
    title: String,
    category: String,
    tags: Vec<String>,
    summary: String,
}

/// 从模型输出里取出 JSON（容忍 ```json 包裹和前后多余文字）。
fn parse_classified(text: &str) -> Option<Classified> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let s = |k: &str| v[k].as_str().unwrap_or("").trim().to_string();
    let c = Classified {
        title: s("title"),
        category: s("category"),
        tags: v["tags"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.as_str().map(|t| t.trim().to_string())).filter(|t| !t.is_empty()).take(6).collect())
            .unwrap_or_default(),
        summary: s("summary"),
    };
    (!c.category.is_empty()).then_some(c)
}

async fn classify(app: AppHandle, id: i64) {
    let Ok(Some(row)) = store::get_collection(&app, id) else { return };
    let _ = store::set_collection_status(&app, id, "classifying");
    let _ = app.emit("collections:changed", id);

    let existing = store::categories(&app).unwrap_or_default();
    let same_source = store::category_of_source(&app, &row.source_key, id).ok().flatten();
    let scene = Scene {
        app_name: row.app_name.clone(),
        window_title: row.window_title.clone(),
        url: row.url.clone(),
        doc_path: row.doc_path.clone(),
        ..Default::default()
    };
    let mut prompt = format!(
        "用户收藏了这张截图，请帮他整理。\n{}\n\n只输出一个 JSON 对象，不要其他文字：\
         {{\"title\": \"不超过 20 字的标题，概括截图里的核心内容\", \
         \"category\": \"分类名，2～6 个字\", \
         \"tags\": [\"最多 5 个关键词\"], \
         \"summary\": \"不超过 80 字的摘要\"}}",
        scene.describe()
    );
    if let Some(c) = &same_source {
        prompt.push_str(&format!("\n同一来源之前的收藏归在「{c}」，如无明显不同请沿用。"));
    } else if !existing.is_empty() {
        prompt.push_str(&format!("\n已有分类：{}。合适就复用，不合适再新建。", existing.join("、")));
    }

    let image_url = match scene::image_data_url_for_llm(Path::new(&row.image_path)) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("收藏：读取截图失败（{id}）：{e}");
            let _ = store::set_collection_status(&app, id, "failed");
            let _ = app.emit("collections:changed", id);
            return;
        }
    };
    let req = Request {
        purpose: Purpose::Classify,
        messages: vec![Message { role: Role::User, parts: vec![Part::Text(prompt), Part::Image(image_url)] }],
        max_tokens: Some(2048),
    };
    match llm::complete(&app, req).await {
        Ok(reply) => match parse_classified(&reply.content) {
            Some(c) => {
                let category = same_source.unwrap_or(c.category);
                let title = if c.title.is_empty() { row.title.clone() } else { c.title };
                let _ = store::classify_collection(&app, id, &title, &category, &c.tags, &c.summary, "done");
                log::info!("收藏：{id} 归类为「{category}」，标题「{title}」");
            }
            None => {
                log::warn!("收藏：{id} 归类结果无法解析：{}", reply.content.chars().take(200).collect::<String>());
                let _ = store::set_collection_status(&app, id, "failed");
            }
        },
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
    tauri::async_runtime::spawn(async move { classify(app, id).await });
}

#[tauri::command]
pub fn collection_open_source(app: AppHandle, id: i64) -> Result<(), String> {
    let c = store::get_collection(&app, id)?.ok_or("收藏不存在")?;
    let target = c.url.clone().or_else(|| c.doc_path.clone());
    open_with(c.app_path.as_deref(), target.as_deref())
}

#[tauri::command]
pub fn usage_today(app: AppHandle) -> Result<store::UsageSummary, String> {
    let day = 24 * 3600 * 1000;
    let now = store::now_ms();
    store::usage_since(&app, now - now % day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_reply() {
        let t = "好的：\n```json\n{\"title\":\"Rust 生命周期\",\"category\":\"编程\",\"tags\":[\"Rust\",\" \"],\"summary\":\"讲解\"}\n```";
        let c = parse_classified(t).unwrap();
        assert_eq!(c.category, "编程");
        assert_eq!(c.tags, vec!["Rust"]);
        assert!(parse_classified("没有 JSON").is_none());
    }
}
