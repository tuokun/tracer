//! Owner task —— 计时与落库的唯一写者（Q4）。
//!
//! 所有事件经 mpsc channel 串行进入此 task；DB `Connection` 也归属此 task。
//! 跨线程零共享可变状态，避免 hook 线程与 flush 兜底之间的竞态（评审一·6）。
//!
//! flush 语义（评审一·1、一·5）：
//! - 切换窗口 → 当前段立即落库，开新段（切换=活跃，不做 idle 丢弃）。
//! - FlushTick → 读 idle+audio：`idle≥阈值 && !playing` 则整段丢弃，否则 checkpoint 落库并重置 start。
//! - PowerSuspend → 落库后清段；PowerResume → 清段，等下次切换重建（已知小缺口，v1 接受）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{error, info};

use crate::core::types::CurrentSession;
use crate::core::{audio, config, db, event::Event, idle, repo};

/// 当前正在计时的前台段。
struct Segment {
    app_id: i64,
    start: i64, // unix 秒
}

/// 在 Tauri 异步运行时上启动 owner task，串行消费所有事件。
/// `session_state` 供 Tauri 命令读取当前前台会话。
/// `last_active` 记录最近的非 tracer 应用。
pub fn spawn(
    mut rx: UnboundedReceiver<Event>,
    conn: Connection,
    session_state: Arc<Mutex<Option<CurrentSession>>>,
    last_active: Arc<Mutex<Option<CurrentSession>>>,
) {
    let cfg = config::load(&conn);
    if let Ok(n) = db::table_count(&conn) {
        info!(
            tables = n,
            version = db::version(&conn).unwrap_or(0),
            flush_interval_secs = cfg.flush_interval_secs,
            "数据库就绪"
        );
    }

    tauri::async_runtime::spawn(async move {
        let idle_threshold_ms = cfg.idle_threshold_ms();
        let mut current: Option<Segment> = None;
        let mut flush_tick = tokio::time::interval(Duration::from_secs(cfg.flush_interval_secs));

        loop {
            tokio::select! {
                _ = flush_tick.tick() => {
                    let idle = idle::idle_ms().unwrap_or(0);
                    let playing = audio::is_playing();
                    if let Some(seg) = current.as_mut() {
                        let now = now_unix();
                        let duration = now - seg.start;
                        if duration > 0 {
                            if idle >= idle_threshold_ms && !playing {
                                info!(idle_ms = idle, dur = duration, "整段丢弃（空闲）");
                            } else {
                                match repo::add_duration(&conn, seg.app_id, seg.start, duration) {
                                    Ok(()) => info!(idle_ms = idle, dur = duration, "flush 落库"),
                                    Err(e) => error!(dur = duration, "flush 写库失败: {e}"),
                                }
                            }
                            seg.start = now; // 丢弃或 checkpoint 后都重置起点
                        }
                    }
                }
                ev = rx.recv() => match ev {
                    Some(Event::ForegroundChanged { info, peak }) => {
                        let now = now_unix();
                        // 切换 = 用户活跃，当前段一律落库。
                        if let Some(seg) = current.as_mut() {
                            let duration = now - seg.start;
                            if duration > 0 {
                                if let Err(e) = repo::add_duration(&conn, seg.app_id, seg.start, duration) {
                                    error!("切换写库失败: {e}");
                                }
                            }
                        }
                        let is_tracer = info.name == "tracer.exe";
                        match repo::upsert_app(&conn, &info.name, info.display_name.as_deref(), Some(&info.path)) {
                            Ok(app_id) => {
                                let process_name = info.name.clone();
                                current = Some(Segment { app_id, start: now });
                                // 更新 session state
                                if let Ok(mut s) = session_state.lock() {
                                    *s = Some(CurrentSession {
                                        process_name: process_name.clone(),
                                        display_name: None,
                                        start_timestamp: now,
                                        current_duration: 0,
                                    });
                                }
                                // 非 tracer 则同时更新 last_active
                                if !is_tracer {
                                    if let Ok(mut s) = last_active.lock() {
                                        *s = Some(CurrentSession {
                                            process_name,
                                            display_name: None,
                                            start_timestamp: now,
                                            current_duration: 0,
                                        });
                                    }
                                }
                                info!(pid = info.pid, name = %info.name, peak = ?peak, "前台切换（已记录）");
                            }
                            Err(e) => {
                                error!("upsert_app 失败: {e}");
                                current = None;
                            }
                        }
                    }
                    Some(Event::PowerSuspend) => {
                        if let Some(seg) = current.as_mut() {
                            let now = now_unix();
                            let duration = now - seg.start;
                            if duration > 0 {
                                if let Err(e) = repo::add_duration(&conn, seg.app_id, seg.start, duration) {
                                    error!("挂起写库失败: {e}");
                                }
                            }
                        }
                        current = None;
                        if let Ok(mut s) = session_state.lock() {
                            *s = None;
                        }
                        info!("系统挂起（已落库，暂停计时）");
                    }
                    Some(Event::PowerResume) => {
                        current = None; // 唤醒后等下一次前台切换重建段
                        info!("系统恢复（恢复计时）");
                    }
                    None => break,
                }
            }
        }
        drop(conn);
    });
}

fn now_unix() -> i64 {
    chrono::Local::now().timestamp()
}
