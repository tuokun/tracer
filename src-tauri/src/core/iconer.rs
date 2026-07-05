use std::mem::size_of;
use std::path::Path;

use tracing::{error, warn};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, RGBQUAD,
};
use windows::Win32::UI::Shell::ExtractAssociatedIconW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO,
};

/// RAII：作用域结束自动 DeleteObject。
struct HbitmapGuard(HBITMAP);
impl Drop for HbitmapGuard {
    fn drop(&mut self) {
        unsafe { let _ = DeleteObject(self.0.into()); }
    }
}

/// RAII：作用域结束自动 DeleteDC。
struct DcGuard(HDC);
impl Drop for DcGuard {
    fn drop(&mut self) {
        unsafe { let _ = DeleteDC(self.0); }
    }
}

pub fn extract(exe_path: &str, store_dir: &Path, stem: &str) -> Option<String> {
    let icon_path = store_dir.join(format!("{}.png", stem));
    std::fs::create_dir_all(store_dir).ok()?;

    unsafe {
        let mut buf = [0u16; 128];
        let wide: Vec<u16> = exe_path.encode_utf16().collect();
        let copy_len = wide.len().min(127);
        buf[..copy_len].copy_from_slice(&wide[..copy_len]);
        buf[copy_len] = 0;

        let mut icon_idx = 0u16;
        let hicon = ExtractAssociatedIconW(Some(HINSTANCE::default()), &mut buf, &mut icon_idx);
        if hicon.0.is_null() {
            warn!(exe = exe_path, "无关联图标");
            return None;
        }

        let result = save_hicon_as_png(hicon, &icon_path);
        let _ = DestroyIcon(hicon);
        match result {
            Ok(()) => {
                tracing::info!(exe = exe_path, "图标已提取");
                Some(icon_path.to_string_lossy().to_string())
            }
            Err(e) => {
                error!(exe = exe_path, "图标保存失败: {e}");
                None
            }
        }
    }
}

/// 直接读取图标自带的 32bpp hbmColor 位图（保留 alpha 通道）。
/// 比 DrawIconEx 绘制到 compatible bitmap 更可靠——后者会丢失 alpha，导致 PNG 透明。
unsafe fn save_hicon_as_png(hicon: HICON, dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut info: ICONINFO = std::mem::zeroed();
    GetIconInfo(hicon, &mut info)?;
    let _mask = HbitmapGuard(info.hbmMask);
    let color = HbitmapGuard(info.hbmColor);

    let mut bm: BITMAP = std::mem::zeroed();
    if GetObjectW(
        color.0.into(),
        size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut _),
    ) == 0
    {
        return Err("GetObjectW 失败".into());
    }
    let w = bm.bmWidth;
    let h = bm.bmHeight;
    if w <= 0 || h <= 0 {
        return Err("无效图标尺寸".into());
    }

    let dc = DcGuard(CreateCompatibleDC(None));

    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // 负值=自上而下，匹配 image crate 期望
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default(); 1],
    };

    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
    let got = GetDIBits(
        dc.0,
        color.0,
        0,
        h as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut bi,
        DIB_RGB_COLORS,
    );
    if got == 0 {
        return Err("GetDIBits 失败".into());
    }

    // 判断图标是否自带有效 alpha 通道。
    let has_alpha = pixels.chunks(4).any(|c| c[3] != 0);
    // BGRA → RGBA；alpha 无效时（旧式图标）按"非黑像素=不透明"补全。
    for chunk in pixels.chunks_mut(4) {
        let b = chunk[0];
        let g = chunk[1];
        let r = chunk[2];
        let a = if has_alpha {
            chunk[3]
        } else if (r | g | b) != 0 {
            255
        } else {
            0
        };
        chunk[0] = r;
        chunk[1] = g;
        chunk[2] = b;
        chunk[3] = a;
    }

    let img = image::RgbaImage::from_raw(w as u32, h as u32, pixels)
        .ok_or("RgbaImage::from_raw 失败")?;
    img.save(dest)?;
    Ok(())
}
