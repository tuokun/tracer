//! 前台窗口焦点监听 —— 阶段一 S1/S5
//!
//! 专用 OS 线程内注册 `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` 并跑 `GetMessage`
//! 消息循环。回调解析进程后把 `Event` 投递进 mpsc channel——hook 线程不碰计时状态，
//! 只投递事件，由 owner task 消费（Q4 线程模型，见 `docs/方案评审记录.md` 一·6）。
//!
//! 线程模型：hook 回调由“注册它的线程的消息循环”派发，因此必须在专用 OS 线程内
//! 注册并泵消息——不能放进会阻塞的 tokio 任务。

use std::thread;

use crate::core::{audio, event::{self, Event}, process};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetMessageW, MSG, WINEVENT_OUTOFCONTEXT,
};

/// 启动前台窗口监听线程。`tx` 在线程启动时存入 thread_local，供回调投递事件。
pub fn spawn(tx: UnboundedSender<Event>) -> thread::JoinHandle<()> {
    thread::spawn(move || unsafe {
        event::set_sender(tx);
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if hook.0.is_null() {
            error!("SetWinEventHook 失败");
            return;
        }
        info!("win event hook 已注册，进入消息循环");

        let mut msg = MSG::default();
        // GetMessageW 收到 WM_QUIT 时返回 false；此后 hook drop，
        // 由 windows_core::Free 自动调用 UnhookWinEvent。
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    })
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _ev: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _id_thread: u32,
    _time: u32,
) {
    // 仅关心窗口本体（OBJID_WINDOW == 0），忽略滚动条/子对象等通知。
    if id_object != 0 {
        return;
    }
    let Some(info) = process::resolve(hwnd) else {
        tracing::warn!(hwnd = hwnd.0 as isize, "前台窗口切换（进程解析失败，已丢弃）");
        return;
    };
    event::send(Event::ForegroundChanged {
        info,
        peak: audio::peak(),
    });
}
