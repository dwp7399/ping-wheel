//! 信号轮盘：按 ⌘⇧C 唤出，鼠标指向某项即高亮，左键点击执行。
//! 右键、Esc 或再按一次 ⌘⇧C 取消。轮盘打开期间的鼠标点击会被拦截，不会落到下面的应用。
//! 每项有固定方位，选中范围按角度最近划分；中心死区为菜单。

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::input;

/// 轮盘窗口边长（逻辑像素），需与 tauri.conf.json 中 wheel 窗口、src/Wheel.tsx 一致。
const WHEEL_SIZE: f64 = 440.0;
/// 中心死区半径（逻辑像素），在此范围内视为选中中间。需与 src/types.ts 一致。
const DEAD_ZONE: f64 = 30.0;
/// 没有事件监听权限时轮询光标的间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(8);

/// 当前选择：None 表示中心（菜单），Some(i) 表示第 i 个方向项。
pub type Selection = Option<usize>;

#[derive(Default)]
struct Inner {
    open: bool,
    /// 唤出时光标的全局逻辑坐标（点）
    origin: (f64, f64),
    /// 本次唤出时各项的方位
    angles: Vec<f64>,
    selection: Selection,
    /// 每次唤出递增，用于让旧的轮询线程退出
    session: u64,
}

#[derive(Default)]
pub struct WheelState(Mutex<Inner>);

#[derive(Clone, Serialize)]
struct MovePayload {
    dx: f64,
    dy: f64,
    index: Selection,
}

pub fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyC)
}

fn escape_shortcut() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

pub fn is_open(app: &AppHandle) -> bool {
    app.state::<WheelState>().0.lock().unwrap().open
}

/// 唤出点（全局逻辑坐标），供提示窗等定位使用。
pub fn anchor(app: &AppHandle) -> (f64, f64) {
    app.state::<WheelState>().0.lock().unwrap().origin
}

/// 光标的全局逻辑坐标（点）。macOS 直接读系统坐标；其他平台由物理坐标按所在屏幕缩放换算。
fn cursor_point(app: &AppHandle) -> Option<(f64, f64)> {
    if let Some(p) = input::cursor_location() {
        return Some(p);
    }
    let p = app.cursor_position().ok()?;
    let scale = app
        .monitor_from_point(p.x, p.y)
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    Some((p.x / scale, p.y / scale))
}

/// 全局快捷键回调入口。
pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }
    if *shortcut == escape_shortcut() {
        log::info!("轮盘：Esc 取消");
        close(app);
        return;
    }
    if *shortcut != toggle_shortcut() {
        return;
    }
    if is_open(app) {
        log::info!("轮盘：再按快捷键，取消");
        close(app);
    } else {
        open_wheel(app);
    }
}

/// 光标移动到全局坐标 (x, y)。
pub fn on_pointer_move(app: &AppHandle, x: f64, y: f64) {
    let payload = {
        let st = app.state::<WheelState>();
        let mut g = st.0.lock().unwrap();
        if !g.open {
            return;
        }
        let (dx, dy) = (x - g.origin.0, y - g.origin.1);
        let index = selection_for(dx, dy, &g.angles);
        g.selection = index;
        MovePayload { dx, dy, index }
    };
    let _ = app.emit_to("wheel", "wheel:move", payload);
}

/// 左键点击：执行当前选中项。
pub fn on_click(app: &AppHandle) {
    if is_open(app) {
        confirm(app);
    }
}

/// 右键点击：取消。
pub fn on_cancel_click(app: &AppHandle) {
    if is_open(app) {
        log::info!("轮盘：右键取消");
        close(app);
    }
}

fn open_wheel(app: &AppHandle) {
    let Some(win) = app.get_webview_window("wheel") else {
        log::error!("轮盘：找不到 wheel 窗口");
        return;
    };
    let Some((x, y)) = cursor_point(app) else {
        log::error!("轮盘：读取光标位置失败");
        return;
    };
    let half = WHEEL_SIZE / 2.0;
    if let Err(e) = win.set_position(LogicalPosition::new(x - half, y - half)) {
        log::warn!("轮盘：定位窗口失败（光标 {x:.0},{y:.0}）：{e}");
    }

    let angles: Vec<f64> = {
        let cfg = app.state::<crate::config::ConfigState>();
        let cfg = cfg.0.lock().unwrap();
        cfg.items.iter().map(|it| it.angle).collect()
    };
    let count = angles.len();
    let session = {
        let st = app.state::<WheelState>();
        let mut g = st.0.lock().unwrap();
        g.open = true;
        g.origin = (x, y);
        g.angles = angles;
        g.selection = None;
        g.session += 1;
        g.session
    };

    let tap = input::tap_active();
    let _ = win.emit("wheel:open", tap);
    // 有事件监听时由它拦截点击，窗口完全穿透；否则让窗口自己接收点击
    let _ = win.set_ignore_cursor_events(tap);
    let _ = win.show();
    if let Err(e) = app.global_shortcut().register(escape_shortcut()) {
        log::warn!("轮盘：注册 Esc 失败：{e}");
    }
    log::info!(
        "轮盘：唤出，光标 ({x:.0},{y:.0})，{count} 项，跟踪方式 {}",
        if tap { "事件监听" } else { "轮询" }
    );

    if !tap {
        let app = app.clone();
        thread::spawn(move || poll_cursor(app, session));
    }
}

/// 没有事件监听权限时轮询光标位置。
fn poll_cursor(app: AppHandle, session: u64) {
    loop {
        thread::sleep(POLL_INTERVAL);
        {
            let st = app.state::<WheelState>();
            let g = st.0.lock().unwrap();
            if !g.open || g.session != session {
                return;
            }
        }
        if let Some((x, y)) = cursor_point(&app) {
            on_pointer_move(&app, x, y);
        }
    }
}

/// 两个方位之间的夹角（0～180）。
fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// 根据位移算出选中项：死区内为中心；否则取方位离光标方向最近的一项。
fn selection_for(dx: f64, dy: f64, angles: &[f64]) -> Selection {
    if angles.is_empty() || dx.hypot(dy) < DEAD_ZONE {
        return None;
    }
    let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    angles
        .iter()
        .enumerate()
        .min_by(|a, b| angle_diff(angle, *a.1).total_cmp(&angle_diff(angle, *b.1)))
        .map(|(i, _)| i)
}

fn confirm(app: &AppHandle) {
    let selection = app.state::<WheelState>().0.lock().unwrap().selection;
    close(app);
    let item = selection.and_then(|i| {
        let cfg = app.state::<crate::config::ConfigState>();
        let cfg = cfg.0.lock().unwrap();
        cfg.items.get(i).cloned()
    });
    log::info!(
        "轮盘：执行「{}」",
        item.as_ref().map(|it| it.label.as_str()).unwrap_or("菜单")
    );
    crate::actions::dispatch(app, item);
}

fn close(app: &AppHandle) {
    app.state::<WheelState>().0.lock().unwrap().open = false;
    if let Some(win) = app.get_webview_window("wheel") {
        let _ = win.hide();
    }
    let _ = app.global_shortcut().unregister(escape_shortcut());
}

/// 无事件监听时，轮盘窗口自己收到的点击（button：0 左键，2 右键）。
#[tauri::command]
pub fn wheel_click(app: AppHandle, button: u8) {
    match button {
        0 => on_click(&app),
        _ => on_cancel_click(&app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOUR: [f64; 4] = [0.0, 90.0, 180.0, 270.0];

    fn toward(deg: f64, angles: &[f64]) -> Selection {
        let r = deg.to_radians();
        selection_for(100.0 * r.sin(), -100.0 * r.cos(), angles)
    }

    #[test]
    fn four_items() {
        assert_eq!(selection_for(0.0, 0.0, &FOUR), None);
        assert_eq!(selection_for(10.0, -10.0, &FOUR), None);
        assert_eq!(selection_for(5.0, -60.0, &FOUR), Some(0));
        assert_eq!(selection_for(80.0, -20.0, &FOUR), Some(1));
        assert_eq!(selection_for(-5.0, 60.0, &FOUR), Some(2));
        assert_eq!(selection_for(-80.0, 10.0, &FOUR), Some(3));
    }

    #[test]
    fn insert_only_splits_neighbors() {
        // 在左上（315°）加一项：只切分左与上之间，右、下不受影响
        let a = [0.0, 90.0, 180.0, 270.0, 315.0];
        assert_eq!(toward(300.0, &a), Some(4));
        assert_eq!(toward(345.0, &a), Some(0));
        assert_eq!(toward(260.0, &a), Some(3));
        assert_eq!(toward(46.0, &a), Some(1));
        assert_eq!(toward(134.0, &a), Some(1));
        assert_eq!(toward(225.5, &a), Some(3));

        // 再在左上与左之间（292.5°）加一项
        let a = [0.0, 90.0, 180.0, 270.0, 315.0, 292.5];
        assert_eq!(toward(292.0, &a), Some(5));
        assert_eq!(toward(310.0, &a), Some(4));
        assert_eq!(toward(275.0, &a), Some(3));
    }

    #[test]
    fn single_item() {
        assert_eq!(selection_for(-50.0, 40.0, &[270.0]), Some(0));
    }
}
