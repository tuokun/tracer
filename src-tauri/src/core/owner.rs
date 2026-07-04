//! Owner task —— 计时与落库的唯一写者（Q4）。阶段一只结构化打印事件。
//!
//! 所有事件经 mpsc channel 串行进入此 task 处理，避免 hook 线程与 flush 兜底之间的竞态。
//! 阶段二起在此消费事件、累加 `_appDuration`、写库。

use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;

use crate::core::{event::Event, idle};

/// 在 Tauri 异步运行时上启动 owner task，串行消费所有事件。
pub fn spawn(mut rx: UnboundedReceiver<Event>) {
    tauri::async_runtime::spawn(async move {
        // 阶段一验证用：每 30s 定时采样 idle（独立于输入，故能观测真实增长）。
        // 阶段二此定时器演化为 FlushTick：到点读 idle_ms 判定整段丢弃 + 落库。
        let mut idle_tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                _ = idle_tick.tick() => {
                    info!(idle_ms = ?idle::idle_ms(), "idle 定时采样");
                }
                ev = rx.recv() => match ev {
                    Some(ev) => log_event(ev),
                    None => break,
                }
            }
        }
    });
}

fn log_event(ev: Event) {
    match ev {
        Event::ForegroundChanged { info, peak } => info!(
            pid = info.pid,
            name = %info.name,
            path = %info.path,
            peak = ?peak,
            idle_ms = ?idle::idle_ms(),
            "前台窗口切换"
        ),
        Event::PowerSuspend => info!("系统挂起（暂停计时）"),
        Event::PowerResume => info!("系统恢复（恢复计时）"),
    }
}
