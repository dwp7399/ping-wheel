//! macOS 底层输入：用 CGEventTap 监听鼠标移动和点击（事件驱动，跟手），
//! 用 CGEvent 读光标全局坐标（无需权限）。
//! 事件监听需要「辅助功能」权限；没有权限时返回 false，由调用方退回轮询。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc;
use std::thread;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use tauri::AppHandle;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static TAP_ACTIVE: AtomicBool = AtomicBool::new(false);
/// 轮盘吞掉了一次按下，对应的抬起也要吞掉
static SWALLOW_UP: AtomicBool = AtomicBool::new(false);

/// 光标的全局坐标（单位：点，原点为主屏左上角）。多屏、不同缩放下都与窗口逻辑坐标一致。
pub fn cursor_location() -> Option<(f64, f64)> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
    let p = CGEvent::new(source).ok()?.location();
    Some((p.x, p.y))
}

pub fn tap_active() -> bool {
    TAP_ACTIVE.load(Ordering::Relaxed)
}

/// 检查辅助功能权限，没有时弹出系统授权提示。
pub fn ensure_accessibility(prompt: bool) -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void)
    }
}

/// 启动鼠标事件监听线程（主动式，可拦截点击）。成功返回 true。
/// 轮盘打开时：移动事件用于高亮，左键执行、右键取消，按下与抬起都被吞掉，不传给下面的应用。
pub fn start_mouse_tap(app: AppHandle) -> bool {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("mouse-tap".into())
        .spawn(move || {
            let tap = CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![
                    CGEventType::MouseMoved,
                    CGEventType::LeftMouseDragged,
                    CGEventType::RightMouseDragged,
                    CGEventType::OtherMouseDragged,
                    CGEventType::LeftMouseDown,
                    CGEventType::LeftMouseUp,
                    CGEventType::RightMouseDown,
                    CGEventType::RightMouseUp,
                ],
                move |_proxy, etype, event| match etype {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        log::warn!("输入：鼠标监听被系统停用（{etype:?}），重新启用");
                        let port = TAP_PORT.load(Ordering::Relaxed);
                        if !port.is_null() {
                            unsafe { CGEventTapEnable(port, true) };
                        }
                        CallbackResult::Keep
                    }
                    CGEventType::LeftMouseDown | CGEventType::RightMouseDown => {
                        if !crate::wheel::is_open(&app) {
                            return CallbackResult::Keep;
                        }
                        let left = matches!(etype, CGEventType::LeftMouseDown);
                        SWALLOW_UP.store(true, Ordering::Relaxed);
                        // 回调要尽快返回，否则全系统输入会卡顿
                        let app = app.clone();
                        thread::spawn(move || {
                            if left {
                                crate::wheel::on_click(&app);
                            } else {
                                crate::wheel::on_cancel_click(&app);
                            }
                        });
                        CallbackResult::Drop
                    }
                    CGEventType::LeftMouseUp | CGEventType::RightMouseUp => {
                        if SWALLOW_UP.swap(false, Ordering::Relaxed) {
                            CallbackResult::Drop
                        } else {
                            CallbackResult::Keep
                        }
                    }
                    _ => {
                        let p = event.location();
                        crate::wheel::on_pointer_move(&app, p.x, p.y);
                        CallbackResult::Keep
                    }
                },
            );
            let tap = match tap {
                Ok(t) => t,
                Err(()) => {
                    let _ = tx.send(false);
                    return;
                }
            };
            let Ok(source) = tap.mach_port().create_runloop_source(0) else {
                let _ = tx.send(false);
                return;
            };
            TAP_PORT.store(tap.mach_port().as_concrete_TypeRef() as *mut c_void, Ordering::Relaxed);
            CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
            tap.enable();
            TAP_ACTIVE.store(true, Ordering::Relaxed);
            let _ = tx.send(true);
            CFRunLoop::run_current();
            TAP_ACTIVE.store(false, Ordering::Relaxed);
            log::warn!("输入：鼠标监听线程退出");
        })
        .expect("spawn mouse-tap thread");
    rx.recv().unwrap_or(false)
}
