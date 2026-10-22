//! 轮盘选中后执行的动作。具体功能在 features.rs；这里负责分发、在后台线程执行，
//! 并在唤出点弹出提示窗，说明执行了什么、结果如何。

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::config::{Action, WheelItem};

const TOAST_W: f64 = 360.0;
const TOAST_H: f64 = 120.0;
const TOAST_DURATION: Duration = Duration::from_millis(2200);
/// 淡出动画时长，之后再真正隐藏窗口（避免下次显示时闪出旧内容）
const TOAST_FADE: Duration = Duration::from_millis(180);

static TOAST_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    /// 已执行
    Done,
    /// 已开始，后台继续处理
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
        record(app, None, Status::Done, t!("打开菜单"));
        return;
    };
    // 截图、读场景、写库都可能耗时，放到后台线程，避免卡住快捷键 / 鼠标事件所在线程
    let app = app.clone();
    std::thread::spawn(move || run(&app, item));
}

fn run(app: &AppHandle, item: WheelItem) {
    // (状态, 说明, 是否弹提示)：问号和对话会直接打开对话窗口，成功时不再弹提示
    let (status, detail, toast) = match &item.action {
        Action::Open { target } => match open_target(app, target) {
            Ok(()) => (Status::Done, t!("已打开 {target}", target = target.trim()), true),
            Err(e) => (Status::Failed, e, true),
        },
        Action::Ask { prompt, mode, custom } => match crate::features::ask(app, prompt, mode, custom) {
            Ok(d) => (Status::Done, d, false),
            Err(e) => (Status::Failed, e, true),
        },
        Action::Chat => match crate::features::chat(app) {
            Ok(d) => (Status::Done, d, false),
            Err(e) => (Status::Failed, e, true),
        },
        Action::Mark => match crate::features::mark(app) {
            Ok(d) => (Status::Done, t!("已标记：{detail}", detail = d), true),
            Err(e) => (Status::Failed, e, true),
        },
        Action::Collect => match crate::features::collect(app) {
            Ok(d) => (Status::Pending, d, true),
            Err(e) => (Status::Failed, e, true),
        },
        Action::Plugin { plugin, action } => match crate::plugins::run(app, plugin, action) {
            Ok(d) => (Status::Pending, d, false),
            Err(e) => (Status::Failed, e, true),
        },
    };
    if matches!(status, Status::Failed) {
        log::warn!("动作：「{}」失败：{detail}", item.label);
    }

    if toast {
        show_toast(
            app,
            ToastPayload {
                icon: item.icon.clone(),
                title: item.label.clone(),
                detail: detail.clone(),
                status: status.clone(),
            },
        );
    }
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

/// 供插件等模块弹提示（status：done / pending / failed）。
pub fn toast(app: &AppHandle, icon: &str, title: &str, detail: &str, status: &str) {
    let status = match status {
        "failed" => Status::Failed,
        "pending" => Status::Pending,
        _ => Status::Done,
    };
    show_toast(app, ToastPayload { icon: icon.into(), title: title.into(), detail: detail.into(), status });
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
        if TOAST_SEQ.load(Ordering::Relaxed) != seq {
            return;
        }
        let Some(win) = app.get_webview_window("toast") else { return };
        let _ = win.emit("toast:hide", ());
        thread::sleep(TOAST_FADE);
        if TOAST_SEQ.load(Ordering::Relaxed) == seq {
            let _ = win.hide();
        }
    });
}

fn open_target(app: &AppHandle, target: &str) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() {
        log::warn!("动作：打开目标为空");
        return Err(t!("还没有设置要打开的目标，请在菜单里填写"));
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
            t!("打开 {target} 失败：{e}", target = target, e = e)
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
