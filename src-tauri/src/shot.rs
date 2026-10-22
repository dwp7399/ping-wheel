//! 截图：截取光标所在的整块屏幕；为问号在标记位置画圈，并裁出标记附近的放大图。
//! 截图只用于发给模型和本地收藏 / 标记，不在对话里展示给用户。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

/// 一次截图：文件路径 + 被截屏幕的全局范围（点）。
pub struct Capture {
    pub path: PathBuf,
    pub origin: (f64, f64),
    pub size: (f64, f64),
}

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

/// 包含 (x, y) 的那块屏幕的全局范围（点）：(x, y, 宽, 高)。
#[cfg(target_os = "macos")]
pub fn display_bounds_at(x: f64, y: f64) -> (f64, f64, f64, f64) {
    use core_graphics::display::CGDisplay;
    let b = CGDisplay::active_displays()
        .ok()
        .and_then(|ids| {
            ids.iter()
                .map(|id| CGDisplay::new(*id).bounds())
                .find(|b| x >= b.origin.x && x < b.origin.x + b.size.width && y >= b.origin.y && y < b.origin.y + b.size.height)
        })
        .unwrap_or_else(|| CGDisplay::main().bounds());
    (b.origin.x, b.origin.y, b.size.width, b.size.height)
}

#[cfg(not(target_os = "macos"))]
pub fn display_bounds_at(_x: f64, _y: f64) -> (f64, f64, f64, f64) {
    (0.0, 0.0, 1920.0, 1080.0)
}

/// 截取包含 (x, y) 的那块屏幕，保存为 PNG。
#[cfg(target_os = "macos")]
pub fn capture_display_at(dir: &Path, x: f64, y: f64) -> Result<Capture, String> {
    let (bx, by, bw, bh) = display_bounds_at(x, y);
    let path = dir.join(format!("{}.png", crate::store::now_ms()));
    let rect = format!("{},{},{},{}", bx as i64, by as i64, bw as i64, bh as i64);
    let status = Command::new("screencapture")
        .args(["-x", "-t", "png", "-R", &rect])
        .arg(&path)
        .status()
        .map_err(|e| format!("调用 screencapture 失败：{e}"))?;
    if !status.success() || !path.exists() {
        return Err("截图失败，请确认已在「系统设置 → 隐私与安全性 → 屏幕录制」里允许 Ping Wheel".into());
    }
    Ok(Capture { path, origin: (bx, by), size: (bw, bh) })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_display_at(_dir: &Path, _x: f64, _y: f64) -> Result<Capture, String> {
    Err("当前平台暂不支持截图".into())
}

fn jpeg_data_url(img: &DynamicImage, max_edge: u32) -> Result<String, String> {
    let (w, h) = img.dimensions();
    let img = if w.max(h) > max_edge { img.resize(max_edge, max_edge, FilterType::Triangle) } else { img.clone() };
    let mut buf = Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 82);
    img.to_rgb8().write_with_encoder(encoder).map_err(|e| format!("编码截图失败：{e}"))?;
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
}

/// 读取截图并压成长边不超过 max_edge 的 JPEG data URL（收藏归类等用）。
pub fn data_url(path: &Path, max_edge: u32) -> Result<String, String> {
    let img = image::open(path).map_err(|e| format!("读取截图失败：{e}"))?;
    jpeg_data_url(&img, max_edge)
}

/// 画标记：白边红圈，圈住标记点但不遮挡正中的内容。
fn draw_marker(img: &mut RgbaImage, cx: f64, cy: f64, scale: f64) {
    let r = 26.0 * scale;
    let red = 3.5 * scale;
    let white = 1.5 * scale;
    let outer = r + red / 2.0 + white;
    let (w, h) = img.dimensions();
    let x0 = (cx - outer).floor().max(0.0) as u32;
    let y0 = (cy - outer).floor().max(0.0) as u32;
    let x1 = ((cx + outer).ceil() as u32).min(w.saturating_sub(1));
    let y1 = ((cy + outer).ceil() as u32).min(h.saturating_sub(1));
    for py in y0..=y1 {
        for px in x0..=x1 {
            let d = ((px as f64 + 0.5 - cx).powi(2) + (py as f64 + 0.5 - cy).powi(2)).sqrt();
            let dr = (d - r).abs();
            if dr <= red / 2.0 {
                img.put_pixel(px, py, Rgba([235, 40, 45, 255]));
            } else if dr <= red / 2.0 + white {
                img.put_pixel(px, py, Rgba([255, 255, 255, 255]));
            }
        }
    }
}

/// 问号用的图片：带标记的全屏图、以标记为中心的放大图，以及标记在全屏图中的位置比例。
pub struct MarkedImages {
    pub full: String,
    pub crop: String,
    pub marker: (f64, f64),
}

pub fn marked_images(cap: &Capture, anchor: (f64, f64)) -> Result<MarkedImages, String> {
    let img = image::open(&cap.path).map_err(|e| format!("读取截图失败：{e}"))?;
    let mut rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let scale = w as f64 / cap.size.0.max(1.0);
    let fx = ((anchor.0 - cap.origin.0) / cap.size.0).clamp(0.0, 1.0);
    let fy = ((anchor.1 - cap.origin.1) / cap.size.1).clamp(0.0, 1.0);
    let (cx, cy) = (fx * w as f64, fy * h as f64);
    draw_marker(&mut rgba, cx, cy, scale);
    let marked = DynamicImage::ImageRgba8(rgba);

    // 放大图：标记周围 960×600 点的区域（超出屏幕时向内平移）
    let cw = ((960.0 * scale) as u32).min(w);
    let ch = ((600.0 * scale) as u32).min(h);
    let left = (cx - cw as f64 / 2.0).clamp(0.0, (w - cw) as f64) as u32;
    let top = (cy - ch as f64 / 2.0).clamp(0.0, (h - ch) as f64) as u32;
    let crop = marked.crop_imm(left, top, cw, ch);

    Ok(MarkedImages { full: jpeg_data_url(&marked, 1600)?, crop: jpeg_data_url(&crop, 1400)?, marker: (fx, fy) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_and_crop() {
        let dir = std::env::temp_dir();
        let path = dir.join("ping-wheel-shot-test.png");
        // 模拟 2 倍屏：1000×600 点 → 2000×1200 像素
        RgbaImage::from_pixel(2000, 1200, Rgba([20, 20, 20, 255])).save(&path).unwrap();
        let cap = Capture { path: path.clone(), origin: (100.0, 0.0), size: (1000.0, 600.0) };
        let m = marked_images(&cap, (600.0, 300.0)).unwrap();
        assert!((m.marker.0 - 0.5).abs() < 1e-9 && (m.marker.1 - 0.5).abs() < 1e-9);
        assert!(m.full.starts_with("data:image/jpeg;base64,"));
        assert!(m.crop.len() > 100);
        // 边角也不越界
        let m = marked_images(&cap, (100.0, 0.0)).unwrap();
        assert_eq!(m.marker, (0.0, 0.0));
        let _ = std::fs::remove_file(path);
    }
}
