//! 系统挂起/唤醒监听 —— 阶段一 S6
//!
//! 创建一个隐藏的顶层窗口接收 `WM_POWERBROADCAST`（message-only 窗口不收广播，故用
//! `WS_POPUP` 无 `WS_VISIBLE` 的隐藏顶层窗口），把 `PowerSuspend` / `PowerResume`
//! 事件投递进 channel（由 owner task 消费）。阶段一只打印，阶段二在此暂停/恢复计时。

use std::mem::size_of;
use std::thread;

use crate::core::event::{self, Event};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PBT_APMRESUMEAUTOMATIC,
    PBT_APMSUSPEND, RegisterClassExW, WINDOW_EX_STYLE, WM_POWERBROADCAST, WNDCLASSEXW, WS_POPUP,
};

/// 启动电源广播监听线程。后台常驻，随进程退出而结束。
pub fn spawn(tx: UnboundedSender<Event>) -> thread::JoinHandle<()> {
    thread::spawn(move || unsafe {
        event::set_sender(tx);

        let hmodule = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                error!("GetModuleHandleW 失败: {e}");
                return;
            }
        };
        let hinstance = HINSTANCE(hmodule.0);

        // 窗口类名（UTF-16，以 null 结尾）。
        let class_name: Vec<u16> = "TracerPowerSink\0".encode_utf16().collect();
        let class_ptr = PCWSTR(class_name.as_ptr());

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_ptr,
            ..Default::default()
        };
        // 注册失败（如类名已注册）不致命，继续创建窗口即可。
        let _ = RegisterClassExW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_ptr,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                error!("CreateWindowExW 失败: {e}");
                return;
            }
        };
        info!(hwnd = hwnd.0 as isize, "电源广播窗口已创建");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, Some(hwnd), 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    })
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_POWERBROADCAST {
        match wparam.0 as u32 {
            PBT_APMSUSPEND => event::send(Event::PowerSuspend),
            PBT_APMRESUMEAUTOMATIC => event::send(Event::PowerResume),
            _ => {}
        }
        return LRESULT(1); // 电源广播返回 TRUE
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
