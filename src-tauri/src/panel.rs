//! 悬浮面板：显示问号的理解结果、对话、插件结果。
//! - 每次打开一个新面板（窗口标签 panel-N），出现在唤出点旁边
//! - 不常驻：失去焦点就关闭，关闭即结束，不保存历史；点「钉住」后保留在桌面上
//! - 打开新面板时，未钉住的旧面板自动关闭
//! - 只显示「思考中」和回答正文，不显示截图和模型的思考过程

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window};

use crate::cloud::{self, Event};

const W: f64 = 440.0;
const H: f64 = 520.0;

#[derive(Clone)]
pub enum Kind {
    /// 问号：首轮请求体（含截图和场景），追问时连同往来一起重发
    Ask { body: Value },
    /// 对话
    Chat,
    /// 插件给出的结果（Markdown），可继续追问（转为对话）
    Result { content: String },
}

impl Kind {
    fn name(&self) -> &'static str {
        match self {
            Kind::Ask { .. } => "ask",
            Kind::Chat => "chat",
            Kind::Result { .. } => "result",
        }
    }
}

#[derive(Clone, Serialize)]
struct Turn {
    role: String,
    content: String,
}

struct Session {
    kind: Kind,
    title: String,
    pinned: bool,
    /// 问号：首个问题之后的往来；对话：全部往来
    turns: Vec<Turn>,
    /// 正在生成的回答（流式累积）
    pending: Option<String>,
    thinking: bool,
    error: Option<String>,
}

static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);
static SEQ: AtomicU64 = AtomicU64::new(0);

fn with_sessions<T>(f: impl FnOnce(&mut HashMap<String, Session>) -> T) -> T {
    let mut g = SESSIONS.lock().unwrap();
    f(g.get_or_insert_with(HashMap::new))
}

/// 面板位置：唤出点右侧，放不下就放左侧，并保持在屏幕内。
fn place(app: &AppHandle) -> (f64, f64) {
    let (x, y) = crate::wheel::anchor(app);
    let (bx, by, bw, bh) = crate::shot::display_bounds_at(x, y);
    let mut px = x + 28.0;
    if px + W > bx + bw - 12.0 {
        px = (x - 28.0 - W).max(bx + 12.0);
    }
    let py = (y - 90.0).clamp(by + 32.0, (by + bh - H - 16.0).max(by + 32.0));
    (px, py)
}

/// 打开一个新面板。start 为 true 时立即开始生成回答（问号）。
pub fn open(app: &AppHandle, kind: Kind, title: &str, start: bool) -> Result<(), String> {
    // 关掉未钉住的旧面板
    let stale: Vec<String> = with_sessions(|m| m.iter().filter(|(_, s)| !s.pinned).map(|(k, _)| k.clone()).collect());
    for label in stale {
        close(app, &label);
    }

    let label = format!("panel-{}", SEQ.fetch_add(1, Ordering::Relaxed) + 1);
    let (x, y) = place(app);
    with_sessions(|m| {
        m.insert(
            label.clone(),
            Session { kind: kind.clone(), title: title.to_string(), pinned: false, turns: vec![], pending: None, thinking: false, error: None },
        )
    });

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(if title.is_empty() { "对话" } else { title })
        .inner_size(W, H)
        .min_inner_size(320.0, 260.0)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(true)
        .build()
        .map_err(|e| {
            with_sessions(|m| m.remove(&label));
            format!("打开面板失败：{e}")
        })?;
    let _ = win.set_focus();
    log::info!("面板：打开 {label}（{}）", kind.name());

    if start {
        let app = app.clone();
        tauri::async_runtime::spawn(respond(app, label));
    }
    Ok(())
}

/// 插件用：在唤出点旁显示一段 Markdown 结果。
pub fn show_result(app: &AppHandle, title: &str, content: &str) -> Result<(), String> {
    open(app, Kind::Result { content: content.to_string() }, title, false)
}

pub fn close(app: &AppHandle, label: &str) {
    with_sessions(|m| m.remove(label));
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.close();
    }
}

#[derive(Clone, Serialize)]
struct PanelEvent<'a> {
    /// thinking | text | done | error
    r#type: &'a str,
    /// 目前为止的完整回答（不是增量，避免前端错过事件）
    text: &'a str,
}

fn emit(app: &AppHandle, label: &str, kind: &str, text: &str) {
    let _ = app.emit_to(label, "panel:event", PanelEvent { r#type: kind, text });
}

/// 生成回答：按面板类型调用后端，把进度推给面板。
async fn respond(app: AppHandle, label: String) {
    let Some((kind, turns)) = with_sessions(|m| {
        m.get_mut(&label).map(|s| {
            s.pending = Some(String::new());
            s.thinking = false;
            s.error = None;
            (s.kind.clone(), s.turns.clone())
        })
    }) else {
        return;
    };

    let on_event = |e: Event<'_>| {
        let text = with_sessions(|m| {
            let s = m.get_mut(&label)?;
            match e {
                Event::Thinking => s.thinking = true,
                Event::Delta(t) => s.pending.get_or_insert_with(String::new).push_str(t),
            }
            s.pending.clone()
        });
        match (text, &e) {
            (Some(_), Event::Thinking) => emit(&app, &label, "thinking", ""),
            (Some(t), Event::Delta(_)) => emit(&app, &label, "text", &t),
            _ => {}
        }
    };

    let result = match &kind {
        Kind::Ask { body } => {
            let mut b = body.clone();
            b["turns"] = json!(turns);
            cloud::ask(&app, &b, on_event).await
        }
        Kind::Chat | Kind::Result { .. } => {
            let mut messages = turns.clone();
            if let Kind::Result { content } = &kind {
                messages.insert(0, Turn { role: "assistant".into(), content: content.clone() });
            }
            cloud::chat(&app, &json!(messages), on_event).await
        }
    };

    match result {
        Ok(text) => {
            with_sessions(|m| {
                if let Some(s) = m.get_mut(&label) {
                    s.pending = None;
                    s.turns.push(Turn { role: "assistant".into(), content: text.clone() });
                }
            });
            emit(&app, &label, "done", &text);
        }
        Err(e) => {
            log::warn!("面板：{label} 生成失败：{e}");
            with_sessions(|m| {
                if let Some(s) = m.get_mut(&label) {
                    s.pending = None;
                    s.error = Some(e.clone());
                    // 失败的追问不计入往来，方便重发
                    if s.turns.last().is_some_and(|t| t.role == "user") {
                        s.turns.pop();
                    }
                }
            });
            emit(&app, &label, "error", &e);
        }
    }
}

#[derive(Serialize)]
pub struct PanelState {
    kind: &'static str,
    title: String,
    pinned: bool,
    /// 对话往来（问号不含首个问题，只从回答开始）
    turns: Vec<Turn>,
    /// 插件结果的正文
    content: Option<String>,
    pending: Option<String>,
    thinking: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn panel_state(window: Window) -> Result<PanelState, String> {
    with_sessions(|m| {
        let s = m.get(window.label()).ok_or("面板已结束")?;
        Ok(PanelState {
            kind: s.kind.name(),
            title: s.title.clone(),
            pinned: s.pinned,
            turns: s.turns.clone(),
            content: match &s.kind {
                Kind::Result { content } => Some(content.clone()),
                _ => None,
            },
            pending: s.pending.clone(),
            thinking: s.thinking,
            error: s.error.clone(),
        })
    })
}

#[tauri::command]
pub fn panel_send(app: AppHandle, window: Window, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("内容为空".into());
    }
    let label = window.label().to_string();
    with_sessions(|m| {
        let s = m.get_mut(&label).ok_or("面板已结束")?;
        if s.pending.is_some() {
            return Err("正在回答，请稍候".to_string());
        }
        s.turns.push(Turn { role: "user".into(), content: text });
        Ok(())
    })?;
    tauri::async_runtime::spawn(respond(app, label));
    Ok(())
}

#[tauri::command]
pub fn panel_pin(window: Window, pinned: bool) {
    with_sessions(|m| {
        if let Some(s) = m.get_mut(window.label()) {
            s.pinned = pinned;
        }
    });
}

#[tauri::command]
pub fn panel_close(app: AppHandle, window: Window) {
    close(&app, window.label());
}
