use std::path::Path;

use tracing::{error, warn};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    GetObjectW, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ, RGBQUAD,
};
use windows::Win32::UI::Shell::ExtractAssociatedIconW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, GetIconInfo, DI_FLAGS, HICON,
};

struct GdiCleanup {
    dc: Option<HDC>,
    bitmaps: Vec<HBITMAP>,
}

// Re-import the internal HDC alias used by GDI functions.
use windows::Win32::Graphics::Gdi::HDC;

impl Drop for GdiCleanup {
    fn drop(&mut self) {
        unsafe {
            for &bmp in &self.bitmaps {
                let _ = DeleteObject(bmp.into());
            }
            if let Some(dc) = self.dc {
                let _ = DeleteDC(dc);
            }
        }
    }
}

pub fn extract(exe_path: &str, store_dir: &Path, stem: &str) -> Option<String> {
    let icon_path = store_dir.join(format!("{}.png", stem));
    if icon_path.exists() {
        return Some(icon_path.to_string_lossy().to_string());
    }
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

unsafe fn save_hicon_as_png(hicon: HICON, dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut cleanup = GdiCleanup { dc: None, bitmaps: vec![] };

    let mut info = std::mem::zeroed();
    GetIconInfo(hicon, &mut info)?;
    let hbm_color = HBITMAP(info.hbmColor.0);
    let hbm_mask = HBITMAP(info.hbmMask.0);
    cleanup.bitmaps.push(hbm_color);
    cleanup.bitmaps.push(hbm_mask);

    let mut bm: BITMAP = std::mem::zeroed();
    let cb = std::mem::size_of::<BITMAP>() as i32;
    let hgdobj: HGDIOBJ = hbm_color.into();
    if GetObjectW(hgdobj, cb, Some(&mut bm as *mut _ as *mut _)) == 0 {
        return Err("GetObjectW 失败".into());
    }

    let w = bm.bmWidth;
    let h = bm.bmHeight;
    if w <= 0 || h <= 0 {
        return Err("无效图标尺寸".into());
    }

    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return Err("CreateCompatibleDC 失败".into());
    }
    cleanup.dc = Some(dc);

    let bmp = CreateCompatibleBitmap(dc, w, h);
    if bmp.is_invalid() {
        return Err("CreateCompatibleBitmap 失败".into());
    }
    cleanup.bitmaps.push(bmp);

    let old = SelectObject(dc, bmp.into());
    let _ = DrawIconEx(dc, 0, 0, hicon, w, h, 0, None, DI_FLAGS(0x0003u32));
    let _ = SelectObject(dc, old);

    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
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

    let row_len = w as usize * 4;
    let mut pixels = vec![0u8; row_len * h as usize];
    GetDIBits(
        dc,
        bmp,
        0,
        h as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut bi,
        DIB_RGB_COLORS,
    );

    for chunk in pixels.chunks_mut(4) {
        let r = chunk[2];
        let g = chunk[1];
        let b = chunk[0];
        chunk[0] = r;
        chunk[1] = g;
        chunk[2] = b;
    }

    let img = image::RgbaImage::from_raw(w as u32, h as u32, pixels)
        .ok_or("RgbaImage::from_raw 失败")?;
    img.save(dest)?;

    drop(cleanup);
    Ok(())
}
