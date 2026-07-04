//! 事件类型 —— Q4 单 owner task 线程模型（见 `docs/方案评审记录.md` 一·6）。
//!
//! hook/power 线程只产生事件投递进 channel，owner task 是唯一消费者（阶段二起也是唯一写者）。

use std::cell::RefCell;

use crate::core::process::ProcessInfo;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug)]
pub enum Event {
    /// 前台窗口切换：进程信息 + 切换瞬间的音频峰值。
    ForegroundChanged {
        info: ProcessInfo,
        peak: Option<f32>,
    },
    /// 系统准备挂起（`PBT_APMSUSPEND`）。
    PowerSuspend,
    /// 系统自动唤醒（`PBT_APMRESUMEAUTOMATIC`）。
    PowerResume,
    // 阶段二扩展：FlushTick
}

thread_local! {
    /// 当前线程的 channel 发送端。各 hook/power 线程启动时各自 set_sender；
    /// 回调在本线程被派发，故用 thread_local 规避 `Sync`。
    pub(crate) static SENDER: RefCell<Option<UnboundedSender<Event>>> = const { RefCell::new(None) };
}

/// 在当前线程登记 channel 发送端（线程启动时调用一次）。
pub(crate) fn set_sender(tx: UnboundedSender<Event>) {
    SENDER.with(|cell| *cell.borrow_mut() = Some(tx));
}

/// 从当前线程投递一个事件。无发送端时静默丢弃。
pub(crate) fn send(event: Event) {
    SENDER.with(|cell| {
        if let Some(tx) = &*cell.borrow() {
            let _ = tx.send(event);
        }
    });
}
