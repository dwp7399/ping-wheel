//! 轮盘选中后执行的动作。问号、标记、收藏、对话目前是占位，后续步骤逐个实现。
//! 每次执行都在唤出点弹出提示窗，说明执行了什么、结果如何。

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::config::{Action, WheelItem};

const TOAST_W: f64 = 360.0;
const TOAST_H: f64 = 120.0;
const TOAST_DURATION: Duration = Duration::from_millis(1800);

static TOAST_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    /// 已执行
    Done,
    /// 功能开发中（占位）
    Pending,
    /// 执行失败
    Failed,
}

#[derive(Clone, Serialize)]
struct ToastPayload {
    icon: String,
    title: String,
    detail: String,
    status: Status,
}

#[derive(Clone, Serialize)]
struct ActionPayload {
    /// None 表示中心菜单
    item: Option<WheelItem>,
    at: u64,
    status: Status,
    detail: String,
}

pub fn dispatch(app: &AppHandle, item: Option<WheelItem>) {
    let Some(item) = item else {
        show_main(app);
        record(app, None, Status::Done, "打开菜单".into());
        return;
    };

    let (status, detail) = match &item.action {
        Action::Open { target } => match open_target(app, target) {
            Ok(()) => (Status::Done, format!("已打开 {}", target.trim())),
            Err(e) => (Status::Failed, e),
        },
        Action::Ask { prompt } if !prompt.trim().is_empty() => {
            (Status::Pending, format!("截图并问：{}（功能开发中）", prompt.trim()))
        }
        Action::Ask { .. } => (Status::Pending, "截图并让 AI 解释（功能开发中）".into()),
        Action::Mark => (Status::Pending, "记下当前场景（功能开发中）".into()),
        Action::Collect => (Status::Pending, "截图收藏，自动归类（功能开发中）".into()),
        Action::Chat => (Status::Pending, "新开一个对话（功能开发中）".into()),
    };

    show_toast(
        app,
        ToastPayload {
            icon: item.icon.clone(),
            title: item.label.clone(),
            detail: detail.clone(),
            status: status.clone(),
        },
    );
    record(app, Some(item), status, detail);
}

/// 记到主窗口的「最近触发」里。
fn record(app: &AppHandle, item: Option<WheelItem>, status: Status, detail: String) {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    if let Err(e) = app.emit_to("main", "action", ActionPayload { item, at, status, detail }) {
        log::warn!("动作：通知主窗口失败：{e}");
    }
}

/// 在唤出点弹出提示窗，一段时间后自动隐藏。
fn show_toast(app: &AppHandle, payload: ToastPayload) {
    let Some(win) = app.get_webview_window("toast") else {
        log::error!("动作：找不到 toast 窗口");
        return;
    };
    let (x, y) = crate::wheel::anchor(app);
    let _ = win.set_position(LogicalPosition::new(x - TOAST_W / 2.0, y - TOAST_H / 2.0));
    let _ = win.set_ignore_cursor_events(true);
    let _ = win.emit("toast:show", payload);
    let _ = win.show();

    let seq = TOAST_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(TOAST_DURATION);
        // 期间又弹了新提示就不隐藏
        if TOAST_SEQ.load(Ordering::Relaxed) == seq {
            if let Some(win) = app.get_webview_window("toast") {
                let _ = win.hide();
            }
        }
    });
}

fn open_target(app: &AppHandle, target: &str) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() {
        log::warn!("动作：打开目标为空");
        return Err("还没有设置要打开的目标，请在菜单里填写".into());
    }
    let result = if target.contains("://") {
        app.opener().open_url(target, None::<&str>)
    } else {
        app.opener().open_path(target, None::<&str>)
    };
    result
        .map(|()| log::info!("动作：已打开 {target}"))
        .map_err(|e| {
            log::warn!("动作：打开 {target} 失败：{e}");
            format!("打开 {target} 失败：{e}")
        })
}

pub fn show_main(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(win) => {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
        None => log::error!("动作：找不到 main 窗口"),
    }
}
