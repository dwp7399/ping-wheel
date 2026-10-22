//! 场景采集：唤出轮盘时记录「用户当时在看什么」——最前面的应用、窗口标题、
//! 浏览器当前链接、文档路径；以及截取光标所在屏幕。
//!
//! 采集在唤出时就在后台开始（浏览器链接要走 AppleScript，较慢），执行动作时再取结果。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Scene {
    pub app_name: String,
    pub app_path: Option<String>,
    pub bundle_id: Option<String>,
    pub pid: Option<i32>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub doc_path: Option<String>,
}

impl Scene {
    /// 归组用的来源键：同一链接 / 同一文件 / 同一窗口的内容放在一起。
    pub fn source_key(&self) -> String {
        if let Some(u) = &self.url {
            return normalize_url(u);
        }
        if let Some(p) = &self.doc_path {
            return format!("file:{p}");
        }
        format!(
            "app:{}:{}",
            self.bundle_id.as_deref().unwrap_or(&self.app_name),
            self.window_title.as_deref().unwrap_or("")
        )
    }

    /// 给模型看的场景描述。
    pub fn describe(&self) -> String {
        let mut s = format!("当前应用：{}", self.app_name);
        if let Some(t) = &self.window_title {
            s.push_str(&format!("\n窗口标题：{t}"));
        }
        if let Some(u) = &self.url {
            s.push_str(&format!("\n网页链接：{u}"));
        }
        if let Some(p) = &self.doc_path {
            s.push_str(&format!("\n文件：{p}"));
        }
        s
    }

    /// 一句话标题。
    pub fn headline(&self) -> String {
        self.window_title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.app_name.clone())
    }
}

/// 清洗窗口标题：去掉零宽字符（如飞书文档的隐形水印）、
/// 浏览器附加的「内存用量高」提示和末尾的应用名。
pub fn clean_title(raw: &str, app_name: &str) -> String {
    let mut t: String = raw
        .chars()
        .filter(|c| {
            !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
        })
        .collect();
    let suffix = format!(" - {app_name}");
    if !app_name.is_empty() && t.ends_with(&suffix) {
        t.truncate(t.len() - suffix.len());
    }
    // Chrome：「标题 - 内存用量高 - 1.2 GB」
    for marker in [" - 内存用量高", " - High memory usage"] {
        if let Some(i) = t.find(marker) {
            t.truncate(i);
        }
    }
    t.trim().to_string()
}

/// 去掉锚点和常见跟踪参数，让同一篇文章的链接归为一组。
pub fn normalize_url(raw: &str) -> String {
    let Ok(mut u) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    u.set_fragment(None);
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !(k.starts_with("utm_") || ["spm", "from", "share_source", "share_medium", "ref", "fbclid", "gclid"].contains(&k.as_str()))
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(kept);
    }
    let s = u.to_string();
    s.trim_end_matches('/').to_string()
}

// ---------- 后台采集 ----------

static PENDING: Mutex<Option<JoinHandle<Scene>>> = Mutex::new(None);

/// 唤出轮盘时调用：在后台开始采集当前场景。
pub fn begin() {
    let handle = thread::spawn(|| {
        let t = Instant::now();
        let s = capture_scene();
        log::info!(
            "场景：采集完成，耗时 {}ms，应用「{}」，窗口「{}」，链接 {}，文件 {}",
            t.elapsed().as_millis(),
            s.app_name,
            s.window_title.as_deref().unwrap_or("-"),
            s.url.as_deref().unwrap_or("-"),
            s.doc_path.as_deref().unwrap_or("-")
        );
        s
    });
    *PENDING.lock().unwrap() = Some(handle);
}

/// 取本次唤出时采集的场景（必要时等它完成）。
pub fn take() -> Scene {
    let handle = PENDING.lock().unwrap().take();
    match handle {
        Some(h) => h.join().unwrap_or_default(),
        None => capture_scene(),
    }
}

/// 运行外部命令，超时则结束它。
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<String> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                log::warn!("场景：命令超时（{:?}）", cmd.get_program());
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        log::warn!("场景：命令失败（{:?}）：{}", cmd.get_program(), err.trim());
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn capture_scene() -> Scene {
    let mut s = frontmost_app().unwrap_or_default();
    if let Some(pid) = s.pid {
        let (title, doc) = ax::focused_window_info(pid);
        s.window_title = title.map(|t| clean_title(&t, &s.app_name)).filter(|t| !t.is_empty());
        s.doc_path = doc;
    }
    if let Some(bid) = s.bundle_id.clone() {
        s.url = browser_url(&bid);
    }
    s
}

#[cfg(not(target_os = "macos"))]
fn capture_scene() -> Scene {
    Scene { app_name: "未知应用".into(), ..Default::default() }
}

/// 用 lsappinfo 读最前面的应用（约 10ms，无需权限）。
#[cfg(target_os = "macos")]
fn frontmost_app() -> Option<Scene> {
    let asn = run_with_timeout(Command::new("lsappinfo").arg("front"), Duration::from_secs(1))?;
    let info = run_with_timeout(
        Command::new("lsappinfo").args(["info", "-only", "name", "-only", "pid", "-only", "bundleid", "-only", "bundlepath", &asn]),
        Duration::from_secs(1),
    )?;
    Some(parse_lsappinfo(&info))
}

fn parse_lsappinfo(info: &str) -> Scene {
    let mut s = Scene::default();
    let quoted = |line: &str| line.split_once('"').and_then(|(_, r)| r.rsplit_once('"')).map(|(v, _)| v.to_string());
    for (i, line) in info.lines().enumerate() {
        let line = line.trim();
        if i == 0 {
            if let Some(name) = line.strip_prefix('"').and_then(|r| r.split_once('"')).map(|(n, _)| n) {
                s.app_name = name.to_string();
            }
        } else if line.starts_with("bundleID=") {
            s.bundle_id = quoted(line);
        } else if line.starts_with("bundle path=") {
            s.app_path = quoted(line);
        } else if let Some(rest) = line.strip_prefix("pid = ") {
            s.pid = rest.split_whitespace().next().and_then(|p| p.parse().ok());
        } else if let Some(rest) = line.strip_prefix("\"LSDisplayName\"=") {
            s.app_name = rest.trim_matches('"').to_string();
        }
    }
    s
}

/// 浏览器当前标签页的链接（AppleScript，首次会请求「自动化」权限）。
#[cfg(target_os = "macos")]
fn browser_url(bundle_id: &str) -> Option<String> {
    const CHROMIUM: &[&str] = &[
        "com.google.Chrome",
        "com.google.Chrome.canary",
        "com.google.Chrome.beta",
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "company.thebrowser.Browser",
        "org.chromium.Chromium",
        "com.vivaldi.Vivaldi",
        "com.operasoftware.Opera",
    ];
    const SAFARI: &[&str] = &["com.apple.Safari", "com.apple.SafariTechnologyPreview"];
    let script = if CHROMIUM.contains(&bundle_id) {
        format!(r#"tell application id "{bundle_id}" to return URL of active tab of front window"#)
    } else if SAFARI.contains(&bundle_id) {
        format!(r#"tell application id "{bundle_id}" to return URL of front document"#)
    } else {
        return None;
    };
    run_with_timeout(Command::new("osascript").args(["-e", &script]), Duration::from_secs(2))
        .filter(|u| u.starts_with("http") || u.starts_with("file:"))
}

/// 辅助功能 API：读取应用焦点窗口的标题和文档路径。
#[cfg(target_os = "macos")]
mod ax {
    use std::ffi::c_void;

    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    type AXUIElementRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> i32;
        fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> i32;
    }

    unsafe fn copy_attr(el: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
        let attr = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(el, attr.as_concrete_TypeRef(), &mut value) };
        (err == 0 && !value.is_null()).then_some(value)
    }

    unsafe fn copy_string(el: AXUIElementRef, name: &str) -> Option<String> {
        let v = unsafe { copy_attr(el, name)? };
        // 只接受 CFString
        if unsafe { core_foundation::base::CFGetTypeID(v) } != CFString::type_id() {
            unsafe { CFRelease(v) };
            return None;
        }
        let s = unsafe { CFString::wrap_under_create_rule(v as CFStringRef) };
        Some(s.to_string())
    }

    /// 返回（窗口标题，文档路径）。
    pub fn focused_window_info(pid: i32) -> (Option<String>, Option<String>) {
        unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return (None, None);
            }
            AXUIElementSetMessagingTimeout(app, 0.5);
            let result = match copy_attr(app, "AXFocusedWindow").or_else(|| copy_attr(app, "AXMainWindow")) {
                Some(win) => {
                    let title = copy_string(win as AXUIElementRef, "AXTitle");
                    let doc = copy_string(win as AXUIElementRef, "AXDocument")
                        .and_then(|u| url::Url::parse(&u).ok())
                        .and_then(|u| u.to_file_path().ok())
                        .map(|p| p.to_string_lossy().into_owned());
                    CFRelease(win);
                    (title, doc)
                }
                None => (None, None),
            };
            CFRelease(app);
            result
        }
    }
}

// ---------- 截图 ----------

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// 是否有屏幕录制权限；没有时请求（系统会弹窗引导去设置）。
pub fn ensure_screen_access() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        if CGPreflightScreenCaptureAccess() {
            return true;
        }
        log::warn!("截图：没有屏幕录制权限，请求授权");
        CGRequestScreenCaptureAccess()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// 截取包含 (x, y) 的那块屏幕，保存为 PNG，返回路径。
#[cfg(target_os = "macos")]
pub fn capture_display_at(dir: &Path, x: f64, y: f64) -> Result<PathBuf, String> {
    use core_graphics::display::CGDisplay;

    let ids = CGDisplay::active_displays().map_err(|e| format!("获取显示器列表失败：{e}"))?;
    let bounds = ids
        .iter()
        .map(|id| CGDisplay::new(*id).bounds())
        .find(|b| x >= b.origin.x && x < b.origin.x + b.size.width && y >= b.origin.y && y < b.origin.y + b.size.height)
        .unwrap_or_else(|| CGDisplay::main().bounds());

    let path = dir.join(format!("{}.png", crate::store::now_ms()));
    let rect = format!(
        "{},{},{},{}",
        bounds.origin.x as i64, bounds.origin.y as i64, bounds.size.width as i64, bounds.size.height as i64
    );
    let status = Command::new("screencapture")
        .args(["-x", "-t", "png", "-R", &rect])
        .arg(&path)
        .status()
        .map_err(|e| format!("调用 screencapture 失败：{e}"))?;
    if !status.success() || !path.exists() {
        return Err("截图失败，请确认已在「系统设置 → 隐私与安全性 → 屏幕录制」里允许 Ping Wheel".into());
    }
    log::info!("截图：已保存 {}（区域 {rect}）", path.display());
    Ok(path)
}

#[cfg(not(target_os = "macos"))]
pub fn capture_display_at(_dir: &Path, _x: f64, _y: f64) -> Result<PathBuf, String> {
    Err("当前平台暂不支持截图".into())
}

/// 把截图压成长边不超过 1600 的 JPEG，返回 data URL，用于发给模型。
pub fn image_data_url_for_llm(path: &Path) -> Result<String, String> {
    let small = path.with_extension("llm.jpg");
    #[cfg(target_os = "macos")]
    {
        let ok = Command::new("sips")
            .args(["-Z", "1600", "-s", "format", "jpeg", "-s", "formatOptions", "80"])
            .arg(path)
            .arg("--out")
            .arg(&small)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            log::warn!("截图：压缩 {} 失败，改用原图", path.display());
        }
    }
    let (file, mime) = if small.exists() { (small.as_path(), "image/jpeg") } else { (path, "image/png") };
    let bytes = std::fs::read(file).map_err(|e| format!("读取截图失败：{e}"))?;
    let _ = std::fs::remove_file(&small);
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalization() {
        assert_eq!(
            normalize_url("https://example.com/a/b/?utm_source=x&id=3#sec"),
            "https://example.com/a/b/?id=3"
        );
        assert_eq!(normalize_url("https://example.com/post/#top"), "https://example.com/post");
    }

    #[test]
    fn title_cleaning() {
        let raw = "\u{200d}\u{200c}\u{2062}\u{feff}wan3.0-0831 - 飞书云文档 - 内存用量高 - 1.2 GB - Google Chrome";
        assert_eq!(clean_title(raw, "Google Chrome"), "wan3.0-0831 - 飞书云文档");
        assert_eq!(clean_title("README.md — ping-wheel", "Code"), "README.md — ping-wheel");
    }

    #[test]
    fn lsappinfo_parsing() {
        let info = "\"Google Chrome\" ASN:0x0-0x1: (in front) \n    bundleID=\"com.google.Chrome\"\n    bundle path=\"/Applications/Google Chrome.app\"\n    pid = 123 !cgsConnection";
        let s = parse_lsappinfo(info);
        assert_eq!(s.app_name, "Google Chrome");
        assert_eq!(s.bundle_id.as_deref(), Some("com.google.Chrome"));
        assert_eq!(s.app_path.as_deref(), Some("/Applications/Google Chrome.app"));
        assert_eq!(s.pid, Some(123));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live_tests {
    use super::*;

    /// 真实采集当前场景和截图，手动运行：cargo test scene_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn scene_live() {
        let t = Instant::now();
        let s = capture_scene();
        println!("场景（{}ms）：{s:#?}\nsource_key = {}", t.elapsed().as_millis(), s.source_key());
        assert!(!s.app_name.is_empty());

        let dir = std::env::temp_dir();
        let p = capture_display_at(&dir, 10.0, 10.0).expect("截图失败");
        let url = image_data_url_for_llm(&p).expect("压缩失败");
        println!("截图 {}，发给模型的大小 {} KB", p.display(), url.len() / 1024);
        let _ = std::fs::remove_file(p);
    }
}
