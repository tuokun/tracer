//! 仓储层 —— 阶段二 P2
//!
//! app 落库与时长累加。核心是 `add_duration`：把一段时间段按**本地小时/天**切分后
//! upsert 进 `hours_log`/`daily_log`，并累加 `apps.total_time`，单事务。
//! 切分逻辑对齐 Tai `Data.UpdateAppDuration`，桶语义见 `方案评审记录.md` 一·1。

use chrono::{Local, TimeZone, Timelike};
use rusqlite::{params, Connection};

/// 落库一个 app（按 `process_name` 去重），返回其 id。
pub fn upsert_app(
    conn: &Connection,
    process_name: &str,
    display_name: Option<&str>,
    executable_path: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO apps(process_name, display_name, executable_path) \
         VALUES (?1, ?2, ?3)",
        params![process_name, display_name, executable_path],
    )?;
    conn.query_row(
        "SELECT id FROM apps WHERE process_name = ?1",
        params![process_name],
        |r| r.get(0),
    )
}

/// 把 [start_unix, start_unix+duration] 这段时长累加进 `hours_log`/`daily_log`/`apps.total_time`。
/// 单事务。`duration_secs <= 0` 时直接返回。
pub fn add_duration(
    conn: &Connection,
    app_id: i64,
    start_unix: i64,
    duration_secs: i64,
) -> rusqlite::Result<()> {
    if duration_secs <= 0 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for (hour_ts, date, secs) in split_hours(start_unix, duration_secs) {
        upsert_hours(&tx, app_id, hour_ts, secs)?;
        upsert_daily(&tx, app_id, date, secs)?;
    }
    tx.execute(
        "UPDATE apps SET total_time = total_time + ?1 WHERE id = ?2",
        params![duration_secs, app_id],
    )?;
    tx.commit()
}

/// 把段切分为若干本地小时桶：(整点时间戳, YYYYMMDD, 该小时内秒数)。
/// 纯函数，单测覆盖（含跨小时、跨天）。
fn split_hours(start_unix: i64, duration_secs: i64) -> Vec<(i64, u32, i64)> {
    if duration_secs <= 0 {
        return vec![];
    }
    let end_unix = start_unix + duration_secs;
    let mut out = Vec::new();
    // 对齐到 start 所在本地小时的整点。
    let mut hour_dt = Local
        .timestamp_opt(start_unix, 0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();
    loop {
        let bucket_start_ts = hour_dt.timestamp();
        let bucket_end_ts = (hour_dt + chrono::Duration::hours(1)).timestamp();
        let seg_start = start_unix.max(bucket_start_ts);
        let seg_end = end_unix.min(bucket_end_ts);
        let secs = seg_end - seg_start;
        if secs > 0 {
            let date: u32 = hour_dt.format("%Y%m%d").to_string().parse().unwrap();
            out.push((bucket_start_ts, date, secs));
        }
        if bucket_end_ts >= end_unix {
            break;
        }
        hour_dt += chrono::Duration::hours(1);
    }
    out
}

fn upsert_hours(conn: &Connection, app_id: i64, data_time: i64, secs: i64) -> rusqlite::Result<()> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT time FROM hours_log WHERE app_id = ?1 AND data_time = ?2",
            params![app_id, data_time],
            |r| r.get(0),
        )
        .ok();
    match existing {
        Some(t) => conn
            .execute(
                "UPDATE hours_log SET time = ?1 WHERE app_id = ?2 AND data_time = ?3",
                params![t + secs, app_id, data_time],
            )
            .map(|_| ()),
        None => conn
            .execute(
                "INSERT INTO hours_log(app_id, data_time, time) VALUES (?1, ?2, ?3)",
                params![app_id, data_time, secs],
            )
            .map(|_| ()),
    }
}

fn upsert_daily(conn: &Connection, app_id: i64, date: u32, secs: i64) -> rusqlite::Result<()> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT time FROM daily_log WHERE app_id = ?1 AND date = ?2",
            params![app_id, date],
            |r| r.get(0),
        )
        .ok();
    match existing {
        Some(t) => conn
            .execute(
                "UPDATE daily_log SET time = ?1 WHERE app_id = ?2 AND date = ?3",
                params![t + secs, app_id, date],
            )
            .map(|_| ()),
        None => conn
            .execute(
                "INSERT INTO daily_log(app_id, date, time) VALUES (?1, ?2, ?3)",
                params![app_id, date, secs],
            )
            .map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db;
    use std::path::Path;

    fn mem() -> Connection {
        db::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn split_single_hour_partial() {
        // 14:30 起 30 分钟 → 单桶 14:00，30min。
        let s = chrono::Local.timestamp_opt(now_at(14, 30), 0).unwrap().timestamp();
        let buckets = split_hours(s, 30 * 60);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].2, 30 * 60);
    }

    #[test]
    fn split_across_two_hours() {
        // 14:30 起 90 分钟 → 14:00(30) + 15:00(60)。
        let s = chrono::Local.timestamp_opt(now_at(14, 30), 0).unwrap().timestamp();
        let buckets = split_hours(s, 90 * 60);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].2, 30 * 60);
        assert_eq!(buckets[1].2, 60 * 60);
    }

    #[test]
    fn add_duration_writes_buckets_and_total() {
        let conn = mem();
        let app = upsert_app(&conn, "demo.exe", None, None).unwrap();
        let s = chrono::Local.timestamp_opt(now_at(14, 30), 0).unwrap().timestamp();
        add_duration(&conn, app, s, 90 * 60).unwrap();

        // hours_log：两行
        let hours: i64 = conn
            .query_row("SELECT COUNT(*) FROM hours_log WHERE app_id=?1", params![app], |r| r.get(0))
            .unwrap();
        assert_eq!(hours, 2);
        // daily_log：一行，90min
        let daily: i64 = conn
            .query_row("SELECT time FROM daily_log WHERE app_id=?1", params![app], |r| r.get(0))
            .unwrap();
        assert_eq!(daily, 90 * 60);
        // total_time
        let total: i64 = conn
            .query_row("SELECT total_time FROM apps WHERE id=?1", params![app], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 90 * 60);
    }

    /// 构造"今天某时某分"的 unix 时间戳（本地），便于跨小时/跨天测试。
    fn now_at(hour: u32, min: u32) -> i64 {
        use chrono::{Datelike, NaiveDate};
        let today = Local::now();
        let nd = NaiveDate::from_ymd_opt(today.year(), today.month(), today.day())
            .unwrap()
            .and_hms_opt(hour, min, 0)
            .unwrap();
        Local.from_local_datetime(&nd).unwrap().timestamp()
    }
}
