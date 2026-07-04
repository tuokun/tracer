//! 空闲时长探测 —— 阶段一 S6
//!
//! `GetLastInputInfo` + `GetTickCount64` → 系统级空闲毫秒（鼠标+键盘均计入）。
//! 仅探测；阶段二 flush 时据 idle_ms 判定整段丢弃（见 `docs/方案评审记录.md` 一·5）。

use std::mem::size_of;

use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// 自上次输入以来的空闲毫秒。调用失败返回 `None`，绝不 panic。
pub fn idle_ms() -> Option<u64> {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut info).as_bool() {
            return None;
        }
        Some(GetTickCount64().saturating_sub(info.dwTime as u64))
    }
}
