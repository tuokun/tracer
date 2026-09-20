//! SQLite 仓储层。
//!
//! `usage_segments` 是可同步的不可变事实；小时、每日和累计值都是可重建缓存。
//! 本机片段写入与三个缓存更新必须位于同一事务中。

use std::collections::HashMap;

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::core::db;
use crate::core::types::{
    AppItem, AppRankItem, CategoryItem, PieSlice, RadarPoint, StatsSummary, SyncHistoryItem,
    TodaySummary,
};

const SECS_PER_DAY: i64 = 86_400;
const DISPLAY_NAME_SQL: &str = "COALESCE(a.custom_alias, a.system_display_name, a.process_name)";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRecord {
    pub origin_device_id: String,
    pub segment_sequence: i64,
    pub origin_app_id: i64,
    pub process_name: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub source_local_date: i64,
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub device_id: String,
    pub display_name: String,
    pub metadata_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMetadataRecord {
    pub origin_device_id: String,
    pub origin_app_id: i64,
    pub process_name: String,
    pub system_display_name: Option<String>,
    pub custom_alias: Option<String>,
    pub category_sync_id: Option<String>,
    pub is_ignored: bool,
    pub metadata_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryRecord {
    pub sync_id: String,
    pub name: String,
    pub color: Option<String>,
    pub rules: Option<String>,
    pub logical_revision: i64,
    pub revision_device_id: String,
    pub is_deleted: bool,
}

fn normalize_alias(alias: Option<&str>) -> Option<String> {
    alias
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn config_sequence(tx: &Transaction<'_>, key: &str) -> rusqlite::Result<i64> {
    let next: i64 = tx.query_row(
        "SELECT CAST(value AS INTEGER) FROM config WHERE key=?1",
        [key],
        |r| r.get(0),
    )?;
    tx.execute(
        "UPDATE config SET value=CAST(?1 AS TEXT) WHERE key=?2",
        params![next + 1, key],
    )?;
    Ok(next)
}

fn bump_local_metadata(tx: &Transaction<'_>, device_id: &str) -> rusqlite::Result<i64> {
    tx.execute(
        "UPDATE devices SET metadata_revision=metadata_revision+1 WHERE device_id=?1",
        [device_id],
    )?;
    tx.query_row(
        "SELECT metadata_revision FROM devices WHERE device_id=?1",
        [device_id],
        |r| r.get(0),
    )
}

/// 在本设备内按忽略大小写的进程名保存应用，返回本地行 id。
pub fn upsert_app(
    conn: &Connection,
    process_name: &str,
    system_display_name: Option<&str>,
    executable_path: Option<&str>,
) -> rusqlite::Result<i64> {
    let device_id = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    let existing: Option<(i64, Option<String>)> = tx
        .query_row(
            "SELECT id,system_display_name FROM apps WHERE origin_device_id=?1 AND process_name=?2",
            params![device_id, process_name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let id = if let Some((id, old_name)) = existing {
        let changed =
            system_display_name.is_some() && system_display_name.map(str::to_owned) != old_name;
        let revision = if changed {
            Some(bump_local_metadata(&tx, &device_id)?)
        } else {
            None
        };
        tx.execute(
            "UPDATE apps SET system_display_name=COALESCE(?1,system_display_name), executable_path=COALESCE(?2,executable_path), metadata_revision=COALESCE(?3,metadata_revision) WHERE id=?4",
            params![system_display_name, executable_path, revision, id],
        )?;
        id
    } else {
        let origin_app_id = config_sequence(&tx, "next_app_sequence")?;
        let revision = bump_local_metadata(&tx, &device_id)?;
        tx.execute(
            "INSERT INTO apps(origin_device_id,origin_app_id,process_name,system_display_name,executable_path,metadata_revision) VALUES(?1,?2,?3,?4,?5,?6)",
            params![device_id, origin_app_id, process_name, system_display_name, executable_path,revision],
        )?;
        tx.last_insert_rowid()
    };
    tx.commit()?;
    Ok(id)
}

/// 写入本机时长。跨来源本地午夜时拆为多个事实片段。
pub fn add_duration(
    conn: &Connection,
    app_id: i64,
    start_unix: i64,
    duration_secs: i64,
) -> rusqlite::Result<()> {
    if duration_secs <= 0 {
        return Ok(());
    }
    let (ignored, origin_device_id): (bool, String) = conn.query_row(
        "SELECT is_ignored,origin_device_id FROM apps WHERE id=?1",
        [app_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if ignored {
        return Ok(());
    }
    if origin_device_id != db::local_device_id(conn)? {
        return Err(rusqlite::Error::InvalidParameterName(
            "不能为远端来源应用写入本机会话".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    for piece in split_local_days(start_unix, start_unix + duration_secs) {
        let sequence = config_sequence(&tx, "next_segment_sequence")?;
        insert_segment_and_aggregate(
            &tx,
            &origin_device_id,
            sequence,
            app_id,
            piece.start_utc,
            piece.end_utc,
            piece.source_local_date,
            piece.utc_offset_minutes,
        )?;
    }
    tx.commit()
}

#[derive(Debug, Clone, Copy)]
struct LocalDayPiece {
    start_utc: i64,
    end_utc: i64,
    source_local_date: i64,
    utc_offset_minutes: i32,
}

fn date_key(date: NaiveDate) -> i64 {
    date.year() as i64 * 10_000 + date.month() as i64 * 100 + date.day() as i64
}
fn date_from_key(key: i64) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        (key / 10_000) as i32,
        ((key / 100) % 100) as u32,
        (key % 100) as u32,
    )
}
fn local_date_key(ts: i64) -> i64 {
    date_key(Local.timestamp_opt(ts, 0).single().unwrap().date_naive())
}
#[cfg(test)]
fn local_offset_minutes(ts: i64) -> i32 {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .unwrap()
        .offset()
        .local_minus_utc()
        / 60
}
fn local_midnight_timestamp(date: NaiveDate) -> i64 {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp()
}

fn split_local_days(start_utc: i64, end_utc: i64) -> Vec<LocalDayPiece> {
    if end_utc <= start_utc {
        return Vec::new();
    }
    let mut cursor = start_utc;
    let mut pieces = Vec::new();
    while cursor < end_utc {
        let local = Local.timestamp_opt(cursor, 0).single().unwrap();
        let date = local.date_naive();
        let next = local_midnight_timestamp(date.succ_opt().unwrap());
        let piece_end = end_utc.min(next);
        pieces.push(LocalDayPiece {
            start_utc: cursor,
            end_utc: piece_end,
            source_local_date: date_key(date),
            utc_offset_minutes: local.offset().local_minus_utc() / 60,
        });
        cursor = piece_end;
    }
    pieces
}

fn insert_segment_and_aggregate(
    tx: &Transaction<'_>,
    origin_device_id: &str,
    sequence: i64,
    app_id: i64,
    start: i64,
    end: i64,
    day: i64,
    offset: i32,
) -> rusqlite::Result<bool> {
    if end <= start {
        return Ok(false);
    }
    let inserted=tx.execute(
        "INSERT OR IGNORE INTO usage_segments(origin_device_id,segment_sequence,app_id,start_utc,end_utc,source_local_date,utc_offset_minutes) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![origin_device_id,sequence,app_id,start,end,day,offset],
    )?;
    if inserted == 0 {
        let same:bool=tx.query_row(
            "SELECT app_id=?3 AND start_utc=?4 AND end_utc=?5 AND source_local_date=?6 AND utc_offset_minutes=?7 FROM usage_segments WHERE origin_device_id=?1 AND segment_sequence=?2",
            params![origin_device_id,sequence,app_id,start,end,day,offset], |r|r.get(0),
        )?;
        if same {
            return Ok(false);
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "会话唯一键相同但内容不同".into(),
        ));
    }
    let total = end - start;
    for (hour, seconds) in split_hours(start, end) {
        tx.execute(
            "INSERT INTO hours_log(app_id,source_local_date,local_hour,time) VALUES(?1,?2,?3,?4) ON CONFLICT(app_id,source_local_date,local_hour) DO UPDATE SET time=time+excluded.time",
            params![app_id,day,hour,seconds],
        )?;
    }
    tx.execute("INSERT INTO daily_log(app_id,source_local_date,time) VALUES(?1,?2,?3) ON CONFLICT(app_id,source_local_date) DO UPDATE SET time=time+excluded.time", params![app_id,day,total])?;
    tx.execute(
        "UPDATE apps SET total_time=total_time+?1 WHERE id=?2",
        params![total, app_id],
    )?;
    Ok(true)
}

fn split_hours(start: i64, end: i64) -> Vec<(u32, i64)> {
    let mut result = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let local = Local.timestamp_opt(cursor, 0).single().unwrap();
        let hour = local.hour();
        let naive_next =
            local.date_naive().and_hms_opt(hour, 0, 0).unwrap() + chrono::Duration::hours(1);
        let next = Local
            .from_local_datetime(&naive_next)
            .earliest()
            .map(|d| d.timestamp())
            .filter(|ts| *ts > cursor)
            .unwrap_or(cursor + 3600);
        let bucket_end = end.min(next);
        result.push((hour, bucket_end - cursor));
        cursor = bucket_end;
    }
    result
}

/// 幂等导入一条远端事实。只有新插入的事实更新派生统计。
pub fn import_segments(conn: &Connection, records: &[SegmentRecord]) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0;
    for record in records {
        let app = ensure_imported_app(
            &tx,
            &record.origin_device_id,
            record.origin_app_id,
            &record.process_name,
        )?;
        if insert_segment_and_aggregate(
            &tx,
            &record.origin_device_id,
            record.segment_sequence,
            app,
            record.start_utc,
            record.end_utc,
            record.source_local_date,
            record.utc_offset_minutes,
        )? {
            inserted += 1;
        }
    }
    tx.commit()?;
    Ok(inserted)
}

pub fn replace_year(
    conn: &Connection,
    year: i32,
    records: &[SegmentRecord],
) -> rusqlite::Result<usize> {
    let low = year as i64 * 10_000 + 101;
    let high = (year as i64 + 1) * 10_000 + 101;
    if records
        .iter()
        .any(|r| r.source_local_date < low || r.source_local_date >= high)
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "恢复数据包含其他年份".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM usage_segments WHERE source_local_date>=?1 AND source_local_date<?2",
        params![low, high],
    )?;
    let mut inserted = 0;
    for record in records {
        let app = ensure_imported_app(
            &tx,
            &record.origin_device_id,
            record.origin_app_id,
            &record.process_name,
        )?;
        if insert_segment_and_aggregate(
            &tx,
            &record.origin_device_id,
            record.segment_sequence,
            app,
            record.start_utc,
            record.end_utc,
            record.source_local_date,
            record.utc_offset_minutes,
        )? {
            inserted += 1;
        }
    }
    rebuild_derived_tx(&tx)?;
    tx.commit()?;
    Ok(inserted)
}

fn ensure_imported_app(
    tx: &Transaction<'_>,
    device: &str,
    origin_app_id: i64,
    process: &str,
) -> rusqlite::Result<i64> {
    tx.execute(
        "INSERT OR IGNORE INTO devices(device_id,display_name) VALUES(?1,?1)",
        [device],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO apps(origin_device_id,origin_app_id,process_name) VALUES(?1,?2,?3)",
        params![device, origin_app_id, process],
    )?;
    tx.query_row(
        "SELECT id FROM apps WHERE origin_device_id=?1 AND origin_app_id=?2",
        params![device, origin_app_id],
        |r| r.get(0),
    )
}

pub fn export_segments(conn: &Connection, year: i32) -> rusqlite::Result<Vec<SegmentRecord>> {
    let low = year as i64 * 10_000 + 101;
    let high = (year as i64 + 1) * 10_000 + 101;
    let mut stmt=conn.prepare("SELECT s.origin_device_id,s.segment_sequence,a.origin_app_id,a.process_name,s.start_utc,s.end_utc,s.source_local_date,s.utc_offset_minutes FROM usage_segments s JOIN apps a ON a.id=s.app_id WHERE s.source_local_date>=?1 AND s.source_local_date<?2 ORDER BY s.origin_device_id,s.segment_sequence")?;
    let rows = stmt
        .query_map(params![low, high], |r| {
            Ok(SegmentRecord {
                origin_device_id: r.get(0)?,
                segment_sequence: r.get(1)?,
                origin_app_id: r.get(2)?,
                process_name: r.get(3)?,
                start_utc: r.get(4)?,
                end_utc: r.get(5)?,
                source_local_date: r.get(6)?,
                utc_offset_minutes: r.get(7)?,
            })
        })?
        .collect();
    rows
}

#[cfg(test)]
pub fn rebuild_derived(conn: &Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    rebuild_derived_tx(&tx)?;
    tx.commit()
}

fn rebuild_derived_tx(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM hours_log", [])?;
    tx.execute("DELETE FROM daily_log", [])?;
    tx.execute("UPDATE apps SET total_time=0", [])?;
    let rows: Vec<(i64, i64, i64, i64)> = {
        let mut stmt =
            tx.prepare("SELECT app_id,start_utc,end_utc,source_local_date FROM usage_segments")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    for (app, start, end, day) in rows {
        let total = end - start;
        for (hour, seconds) in split_hours(start, end) {
            tx.execute("INSERT INTO hours_log(app_id,source_local_date,local_hour,time) VALUES(?1,?2,?3,?4) ON CONFLICT(app_id,source_local_date,local_hour) DO UPDATE SET time=time+excluded.time",params![app,day,hour,seconds])?;
        }
        tx.execute("INSERT INTO daily_log(app_id,source_local_date,time) VALUES(?1,?2,?3) ON CONFLICT(app_id,source_local_date) DO UPDATE SET time=time+excluded.time",params![app,day,total])?;
        tx.execute(
            "UPDATE apps SET total_time=total_time+?1 WHERE id=?2",
            params![total, app],
        )?;
    }
    Ok(())
}

fn range_date_keys(start: i64, end: i64) -> (i64, i64) {
    (local_date_key(start), local_date_key(end))
}
fn today_key() -> i64 {
    date_key(Local::now().date_naive())
}

pub fn get_today_summary(conn: &Connection) -> rusqlite::Result<TodaySummary> {
    let day = today_key();
    let total:i64=conn.query_row("SELECT COALESCE(SUM(d.time),0) FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date=?1 AND a.is_ignored=0",[day],|r|r.get(0))?;
    let app_count:i64=conn.query_row("SELECT COUNT(DISTINCT LOWER(a.process_name)||CHAR(0)||COALESCE(a.custom_alias,'')) FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date=?1 AND a.is_ignored=0",[day],|r|r.get(0))?;
    let most_used_app=conn.query_row(&format!("SELECT {DISPLAY_NAME_SQL} FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date=?1 AND a.is_ignored=0 GROUP BY LOWER(a.process_name),a.custom_alias ORDER BY SUM(d.time) DESC LIMIT 1"),[day],|r|r.get(0)).optional()?;
    let elapsed =
        (Local::now().timestamp() - local_midnight_timestamp(Local::now().date_naive())).max(0);
    Ok(TodaySummary {
        total_seconds: total,
        most_used_app,
        idle_seconds: (elapsed - total).max(0),
        app_count,
    })
}

pub fn get_app_rank(
    conn: &Connection,
    start_ts: i64,
    end_ts: i64,
    limit: usize,
    device_id: Option<&str>,
) -> rusqlite::Result<Vec<AppRankItem>> {
    let (start, end) = range_date_keys(start_ts, end_ts);
    let total:i64=conn.query_row("SELECT COALESCE(SUM(d.time),0) FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0",params![start,end,device_id],|r|r.get(0))?;
    let mut stmt=conn.prepare(&format!("SELECT MIN(a.process_name),{DISPLAY_NAME_SQL},MAX(a.custom_icon_path),MAX(a.executable_path),SUM(d.time),MAX(c.name),MAX(c.color) FROM daily_log d JOIN apps a ON d.app_id=a.id LEFT JOIN categories c ON a.category_id=c.id AND c.is_deleted=0 WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0 GROUP BY LOWER(a.process_name),a.custom_alias ORDER BY SUM(d.time) DESC LIMIT ?4"))?;
    let rows = stmt
        .query_map(params![start, end, device_id, limit as i64], |r| {
            let seconds: i64 = r.get(4)?;
            Ok(AppRankItem {
                process_name: r.get(0)?,
                display_name: r.get(1)?,
                icon_path: r.get(2)?,
                executable_path: r.get(3)?,
                total_seconds: seconds,
                category_name: r.get(5)?,
                category_color: r.get(6)?,
                percentage: if total > 0 {
                    seconds as f64 / total as f64
                } else {
                    0.0
                },
            })
        })?
        .collect();
    rows
}

pub fn get_hourly_heatmap(conn: &Connection, date_ts: i64) -> rusqlite::Result<Vec<i64>> {
    let mut result = vec![0; 24];
    let mut stmt=conn.prepare("SELECT h.local_hour,SUM(h.time) FROM hours_log h JOIN apps a ON h.app_id=a.id WHERE h.source_local_date=?1 AND a.is_ignored=0 GROUP BY h.local_hour")?;
    for row in stmt.query_map([local_date_key(date_ts)], |r| {
        Ok((r.get::<_, usize>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (hour, seconds) = row?;
        if hour < 24 {
            result[hour] = seconds;
        }
    }
    Ok(result)
}

pub fn get_app_list(
    conn: &Connection,
    search: Option<&str>,
    category_id: Option<i64>,
    sort_by: Option<&str>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
    include_ignored: bool,
) -> rusqlite::Result<Vec<AppItem>> {
    let mut sql=format!("SELECT a.id,a.process_name,{DISPLAY_NAME_SQL},a.executable_path,a.custom_icon_path,{{time}},c.name,c.color,{{last}},a.is_ignored,a.origin_device_id,v.display_name,a.origin_device_id=(SELECT value FROM config WHERE key='local_device_id') FROM apps a {{join}} LEFT JOIN categories c ON a.category_id=c.id AND c.is_deleted=0 JOIN devices v ON v.device_id=a.origin_device_id WHERE 1=1");
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let (Some(s), Some(e)) = (start_ts, end_ts) {
        let (start, end) = range_date_keys(s, e);
        sql=sql.replace("{time}","COALESCE(SUM(d.time),0)").replace("{last}","MAX(d.source_local_date)").replace("{join}","LEFT JOIN daily_log d ON d.app_id=a.id AND d.source_local_date>=? AND d.source_local_date<?");
        values.push(Box::new(start));
        values.push(Box::new(end));
    } else {
        sql = sql
            .replace("{time}", "a.total_time")
            .replace(
                "{last}",
                "(SELECT MAX(source_local_date) FROM daily_log WHERE app_id=a.id)",
            )
            .replace("{join}", "");
    }
    push_app_filters(&mut sql, &mut values, search, category_id, include_ignored);
    if start_ts.is_some() && end_ts.is_some() {
        sql.push_str(" GROUP BY a.id");
    }
    sql.push_str(match sort_by {
        Some("name") => " ORDER BY a.process_name",
        _ => " ORDER BY 6 DESC",
    });
    let refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            Ok(AppItem {
                id: r.get(0)?,
                process_name: r.get(1)?,
                display_name: r.get(2)?,
                executable_path: r.get(3)?,
                icon_path: r.get(4)?,
                total_seconds: r.get(5)?,
                category_name: r.get(6)?,
                category_color: r.get(7)?,
                last_used_date: r.get(8)?,
                is_ignored: r.get(9)?,
                origin_device_id: r.get(10)?,
                device_name: r.get(11)?,
                is_current_device: r.get(12)?,
            })
        })?
        .collect();
    rows
}

fn push_app_filters(
    sql: &mut String,
    values: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    search: Option<&str>,
    category_id: Option<i64>,
    include_ignored: bool,
) {
    if !include_ignored {
        sql.push_str(" AND a.is_ignored=0");
    }
    if let Some(s) = search.filter(|s| !s.is_empty()) {
        sql.push_str(
            " AND (a.process_name LIKE ? OR a.custom_alias LIKE ? OR a.system_display_name LIKE ?)",
        );
        let p = format!("%{s}%");
        values.push(Box::new(p.clone()));
        values.push(Box::new(p.clone()));
        values.push(Box::new(p));
    }
    if let Some(id) = category_id.filter(|id| *id >= 0) {
        sql.push_str(" AND a.category_id=?");
        values.push(Box::new(id));
    }
}

pub fn get_categories(conn: &Connection) -> rusqlite::Result<Vec<CategoryItem>> {
    let mut stmt = conn
        .prepare("SELECT id,name,color,rules FROM categories WHERE is_deleted=0 ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CategoryItem {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                rules: r.get(3)?,
            })
        })?
        .collect();
    rows
}

pub fn save_category(
    conn: &Connection,
    id: Option<i64>,
    name: &str,
    color: Option<&str>,
    rules: Option<&str>,
) -> rusqlite::Result<i64> {
    let device = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    let saved = if let Some(id) = id {
        let rev: i64 = tx.query_row(
            "SELECT logical_revision FROM categories WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        tx.execute("UPDATE categories SET name=?1,color=?2,rules=?3,logical_revision=?4,revision_device_id=?5,is_deleted=0 WHERE id=?6",params![name,color,rules,rev+1,device,id])?;
        id
    } else {
        tx.execute("INSERT INTO categories(sync_id,name,color,rules,logical_revision,revision_device_id) VALUES(?1,?2,?3,?4,1,?5)",params![uuid::Uuid::new_v4().to_string(),name,color,rules,device])?;
        tx.last_insert_rowid()
    };
    tx.commit()?;
    apply_category_rules(conn)?;
    Ok(saved)
}
pub fn delete_category(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    let device = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE categories SET is_deleted=1,logical_revision=logical_revision+1,revision_device_id=?1 WHERE id=?2",params![device,id])?;
    tx.commit()
}
pub fn set_app_category(conn: &Connection, app_id: i64, category_id: i64) -> rusqlite::Result<()> {
    update_local_app_metadata(conn, app_id, "category_id", &category_id)
}

fn update_local_app_metadata<T: rusqlite::types::ToSql>(
    conn: &Connection,
    app_id: i64,
    field: &str,
    value: &T,
) -> rusqlite::Result<()> {
    let device = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    let owner: String = tx.query_row(
        "SELECT origin_device_id FROM apps WHERE id=?1",
        [app_id],
        |r| r.get(0),
    )?;
    if owner != device {
        return Err(rusqlite::Error::InvalidParameterName(
            "不能修改其他设备的应用元数据".into(),
        ));
    }
    let revision = bump_local_metadata(&tx, &device)?;
    tx.execute(
        &format!("UPDATE apps SET {field}=?1,metadata_revision=?2 WHERE id=?3"),
        params![value, revision, app_id],
    )?;
    tx.commit()
}

#[derive(Clone, Copy, Debug)]
pub enum BarGranularity {
    Day,
    Week,
    Month,
    Year,
}
impl BarGranularity {
    pub fn from_str(v: &str) -> Option<Self> {
        match v {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
    pub fn n_buckets(self, start: i64, end: i64) -> usize {
        match self {
            Self::Day => 24,
            Self::Week => 7,
            Self::Month => (Local.timestamp_opt(end, 0).single().unwrap().date_naive()
                - Local.timestamp_opt(start, 0).single().unwrap().date_naive())
            .num_days()
            .max(1) as usize,
            Self::Year => 12,
        }
    }
}

pub fn get_stats_range(
    conn: &Connection,
    g: BarGranularity,
    start_ts: i64,
    end_ts: i64,
    limit: usize,
    device_id: Option<&str>,
) -> rusqlite::Result<Vec<(String, Vec<i64>)>> {
    let n = g.n_buckets(start_ts, end_ts);
    let (start_key, end_key) = range_date_keys(start_ts, end_ts);
    let sql=match g{BarGranularity::Day=>format!("SELECT LOWER(a.process_name),a.custom_alias,{DISPLAY_NAME_SQL},h.source_local_date,h.local_hour,SUM(h.time) FROM hours_log h JOIN apps a ON h.app_id=a.id WHERE h.source_local_date=?1 AND (?2 IS NULL OR a.origin_device_id=?2) AND a.is_ignored=0 GROUP BY LOWER(a.process_name),a.custom_alias,h.local_hour"),_=>format!("SELECT LOWER(a.process_name),a.custom_alias,{DISPLAY_NAME_SQL},d.source_local_date,0,SUM(d.time) FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0 GROUP BY LOWER(a.process_name),a.custom_alias,d.source_local_date")};
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(String, Option<String>, String, i64, usize, i64)> = match g {
        BarGranularity::Day => stmt
            .query_map(params![start_key, device_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?,
        _ => stmt
            .query_map(params![start_key, end_key, device_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?,
    };
    let start_date = date_from_key(start_key).unwrap();
    let mut grouped: HashMap<(String, Option<String>), (String, Vec<i64>, i64)> = HashMap::new();
    for (process, alias, name, day, hour, seconds) in rows {
        let index = match g {
            BarGranularity::Day => hour,
            BarGranularity::Week | BarGranularity::Month => {
                (date_from_key(day).unwrap() - start_date).num_days().max(0) as usize
            }
            BarGranularity::Year => date_from_key(day).unwrap().month0() as usize,
        };
        if index >= n {
            continue;
        }
        let e = grouped
            .entry((process, alias))
            .or_insert_with(|| (name, vec![0; n], 0));
        e.1[index] += seconds;
        e.2 += seconds;
    }
    let mut all: Vec<_> = grouped.into_values().collect();
    all.sort_by(|a, b| b.2.cmp(&a.2));
    let mut output = Vec::new();
    let mut other = vec![0; n];
    for (index, (name, values, _)) in all.into_iter().enumerate() {
        if index < limit {
            output.push((name, values));
        } else {
            for i in 0..n {
                other[i] += values[i];
            }
        }
    }
    if other.iter().any(|v| *v > 0) {
        output.push(("其他".into(), other));
    }
    Ok(output)
}

fn category_stats(
    conn: &Connection,
    start_ts: i64,
    end_ts: i64,
    device_id: Option<&str>,
) -> rusqlite::Result<Vec<(String, Option<String>, i64)>> {
    let (start, end) = range_date_keys(start_ts, end_ts);
    let mut stmt=conn.prepare("SELECT COALESCE(c.name,'未分类'),COALESCE(c.color,'#9A92C8'),SUM(d.time) FROM daily_log d JOIN apps a ON d.app_id=a.id LEFT JOIN categories c ON a.category_id=c.id AND c.is_deleted=0 WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0 GROUP BY COALESCE(c.sync_id,'') ORDER BY 3 DESC")?;
    let rows = stmt
        .query_map(params![start, end, device_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect();
    rows
}
pub fn get_stats_radar(
    conn: &Connection,
    s: i64,
    e: i64,
    device: Option<&str>,
) -> rusqlite::Result<Vec<RadarPoint>> {
    category_stats(conn, s, e, device).map(|v| {
        v.into_iter()
            .map(|(name, color, value)| RadarPoint { name, color, value })
            .collect()
    })
}
pub fn get_stats_pie(
    conn: &Connection,
    s: i64,
    e: i64,
    device: Option<&str>,
) -> rusqlite::Result<Vec<PieSlice>> {
    category_stats(conn, s, e, device).map(|v| {
        v.into_iter()
            .map(|(name, color, value)| PieSlice { name, color, value })
            .collect()
    })
}
pub fn get_stats_summary(
    conn: &Connection,
    start_ts: i64,
    end_ts: i64,
    device_id: Option<&str>,
) -> rusqlite::Result<StatsSummary> {
    let (start, end) = range_date_keys(start_ts, end_ts);
    let total:i64=conn.query_row("SELECT COALESCE(SUM(d.time),0) FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0",params![start,end,device_id],|r|r.get(0))?;
    let cat = category_stats(conn, start_ts, end_ts, device_id)?
        .into_iter()
        .next()
        .map(|v| v.0);
    let app=conn.query_row(&format!("SELECT {DISPLAY_NAME_SQL} FROM daily_log d JOIN apps a ON d.app_id=a.id WHERE d.source_local_date>=?1 AND d.source_local_date<?2 AND (?3 IS NULL OR a.origin_device_id=?3) AND a.is_ignored=0 GROUP BY LOWER(a.process_name),a.custom_alias ORDER BY SUM(d.time) DESC LIMIT 1"),params![start,end,device_id],|r|r.get(0)).optional()?;
    let days = ((end_ts - start_ts) as f64 / SECS_PER_DAY as f64).max(1.0);
    Ok(StatsSummary {
        total_seconds: total,
        most_active_category: cat,
        most_active_app: app,
        daily_average: (total as f64 / days) as i64,
    })
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO config(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
    Ok(())
}
pub fn apply_category_rules(conn: &Connection) -> rusqlite::Result<usize> {
    let cats:Vec<(i64,String)>=conn.prepare("SELECT id,rules FROM categories WHERE is_deleted=0 AND rules IS NOT NULL AND rules!=''")?.query_map([],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let device = db::local_device_id(conn)?;
    let apps:Vec<(i64,String,Option<String>,i64)>=conn.prepare("SELECT id,process_name,COALESCE(custom_alias,system_display_name),category_id FROM apps WHERE origin_device_id=?1")?.query_map([&device],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut updated = 0;
    for (app, process, name, current) in apps {
        if current > 0 {
            continue;
        }
        let mut matched = None;
        for (id, rules) in &cats {
            for rule in rules
                .split([',', ';', '\n'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if glob_match(rule, &process)
                    || name.as_deref().is_some_and(|n| glob_match(rule, n))
                {
                    matched = Some(*id);
                }
            }
        }
        if let Some(id) = matched {
            set_app_category(conn, app, id)?;
            updated += 1;
        }
    }
    Ok(updated)
}
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let mut dp = vec![vec![false; n.len() + 1]; p.len() + 1];
    dp[0][0] = true;
    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=p.len() {
        for j in 1..=n.len() {
            dp[i][j] = match p[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c == n[j - 1],
            };
        }
    }
    dp[p.len()][n.len()]
}

pub fn update_app_display_name(
    conn: &Connection,
    app: i64,
    name: Option<&str>,
) -> rusqlite::Result<()> {
    update_local_app_metadata(conn, app, "custom_alias", &normalize_alias(name))
}
pub fn set_app_ignored(conn: &Connection, app: i64, ignored: bool) -> rusqlite::Result<()> {
    update_local_app_metadata(conn, app, "is_ignored", &ignored)
}
pub fn is_app_ignored(conn: &Connection, app: i64) -> rusqlite::Result<bool> {
    conn.query_row("SELECT is_ignored FROM apps WHERE id=?1", [app], |r| {
        r.get(0)
    })
}
pub fn set_custom_icon_path(
    conn: &Connection,
    app: i64,
    path: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE apps SET custom_icon_path=?1 WHERE id=?2",
        params![path, app],
    )?;
    Ok(())
}
pub fn get_app_icon_info(
    conn: &Connection,
    process: &str,
) -> rusqlite::Result<(i64, Option<String>)> {
    let device = db::local_device_id(conn)?;
    conn.query_row(
        "SELECT id,custom_icon_path FROM apps WHERE origin_device_id=?1 AND process_name=?2",
        params![device, process],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

pub fn list_devices(conn: &Connection) -> rusqlite::Result<Vec<DeviceRecord>> {
    let mut s=conn.prepare("SELECT device_id,display_name,metadata_revision FROM devices ORDER BY display_name,device_id")?;
    let rows = s
        .query_map([], |r| {
            Ok(DeviceRecord {
                device_id: r.get(0)?,
                display_name: r.get(1)?,
                metadata_revision: r.get(2)?,
            })
        })?
        .collect();
    rows
}
pub fn set_local_device_name(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "设备名称不能为空".into(),
        ));
    }
    let device = db::local_device_id(conn)?;
    let tx = conn.unchecked_transaction()?;
    let revision = bump_local_metadata(&tx, &device)?;
    tx.execute(
        "UPDATE devices SET display_name=?1,metadata_revision=?2 WHERE device_id=?3",
        params![name, revision, device],
    )?;
    tx.commit()
}
pub fn record_sync_history(
    conn: &Connection,
    year: i32,
    success: bool,
    uploaded: u64,
    downloaded: u64,
    imported: usize,
    duration_ms: u64,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO sync_history(created_at,year,success,uploaded_bytes,downloaded_bytes,imported_segments,duration_ms,error_summary) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![Local::now().timestamp(),year,success,uploaded as i64,downloaded as i64,imported as i64,duration_ms as i64,error.map(|v|v.chars().take(300).collect::<String>())])?;
    conn.execute("DELETE FROM sync_history WHERE id NOT IN (SELECT id FROM sync_history ORDER BY id DESC LIMIT 10)",[])?;
    Ok(())
}
pub fn available_years(conn: &Connection) -> rusqlite::Result<Vec<i32>> {
    let mut s = conn
        .prepare("SELECT DISTINCT source_local_date/10000 FROM usage_segments ORDER BY 1 DESC")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}
pub fn sync_history(conn: &Connection) -> rusqlite::Result<Vec<SyncHistoryItem>> {
    let mut s=conn.prepare("SELECT created_at,year,success,uploaded_bytes,downloaded_bytes,imported_segments,duration_ms,error_summary FROM sync_history ORDER BY id DESC LIMIT 10")?;
    let rows = s
        .query_map([], |r| {
            Ok(SyncHistoryItem {
                created_at: r.get(0)?,
                year: r.get(1)?,
                success: r.get(2)?,
                uploaded_bytes: r.get(3)?,
                downloaded_bytes: r.get(4)?,
                imported_segments: r.get(5)?,
                duration_ms: r.get(6)?,
                error_summary: r.get(7)?,
            })
        })?
        .collect();
    rows
}
pub fn export_app_metadata(conn: &Connection) -> rusqlite::Result<Vec<AppMetadataRecord>> {
    let mut s=conn.prepare("SELECT a.origin_device_id,a.origin_app_id,a.process_name,a.system_display_name,a.custom_alias,c.sync_id,a.is_ignored,a.metadata_revision FROM apps a LEFT JOIN categories c ON a.category_id=c.id")?;
    let rows = s
        .query_map([], |r| {
            Ok(AppMetadataRecord {
                origin_device_id: r.get(0)?,
                origin_app_id: r.get(1)?,
                process_name: r.get(2)?,
                system_display_name: r.get(3)?,
                custom_alias: r.get(4)?,
                category_sync_id: r.get(5)?,
                is_ignored: r.get(6)?,
                metadata_revision: r.get(7)?,
            })
        })?
        .collect();
    rows
}
pub fn export_categories(conn: &Connection) -> rusqlite::Result<Vec<CategoryRecord>> {
    let mut s=conn.prepare("SELECT sync_id,name,color,rules,logical_revision,revision_device_id,is_deleted FROM categories")?;
    let rows = s
        .query_map([], |r| {
            Ok(CategoryRecord {
                sync_id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                rules: r.get(3)?,
                logical_revision: r.get(4)?,
                revision_device_id: r.get(5)?,
                is_deleted: r.get(6)?,
            })
        })?
        .collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn mem() -> Connection {
        db::open(Path::new(":memory:")).unwrap()
    }
    fn at(h: u32, m: u32) -> i64 {
        let d = Local::now().date_naive().and_hms_opt(h, m, 0).unwrap();
        Local.from_local_datetime(&d).single().unwrap().timestamp()
    }
    #[test]
    fn duration_creates_fact_and_caches() {
        let c = mem();
        let a = upsert_app(&c, "demo.exe", Some("Demo"), None).unwrap();
        add_duration(&c, a, at(14, 30), 5400).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM usage_segments", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT SUM(time) FROM hours_log", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            5400
        );
        assert_eq!(
            c.query_row("SELECT SUM(time) FROM daily_log", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            5400
        );
        assert_eq!(
            c.query_row("SELECT total_time FROM apps WHERE id=?1", [a], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            5400
        );
    }
    #[test]
    fn import_is_idempotent() {
        let c = mem();
        let r = SegmentRecord {
            origin_device_id: "remote".into(),
            segment_sequence: 1,
            origin_app_id: 1,
            process_name: "x.exe".into(),
            start_utc: at(10, 0),
            end_utc: at(10, 1),
            source_local_date: today_key(),
            utc_offset_minutes: local_offset_minutes(at(10, 0)),
        };
        assert_eq!(import_segments(&c, &[r.clone()]).unwrap(), 1);
        assert_eq!(import_segments(&c, &[r]).unwrap(), 0);
        assert_eq!(
            c.query_row("SELECT SUM(time) FROM daily_log", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            60
        );
    }
    #[test]
    fn conflicting_fact_is_rejected() {
        let c = mem();
        let mut r = SegmentRecord {
            origin_device_id: "remote".into(),
            segment_sequence: 1,
            origin_app_id: 1,
            process_name: "x.exe".into(),
            start_utc: at(10, 0),
            end_utc: at(10, 1),
            source_local_date: today_key(),
            utc_offset_minutes: 0,
        };
        import_segments(&c, &[r.clone()]).unwrap();
        r.end_utc += 1;
        assert!(import_segments(&c, &[r]).is_err());
    }
    #[test]
    fn alias_is_trimmed_and_system_name_can_refresh() {
        let c = mem();
        let a = upsert_app(&c, "Chrome.exe", Some("Chrome"), None).unwrap();
        update_app_display_name(&c, a, Some("  谷歌浏览器  ")).unwrap();
        upsert_app(&c, "chrome.EXE", Some("New Chrome"), None).unwrap();
        let v: (Option<String>, Option<String>) = c
            .query_row(
                "SELECT system_display_name,custom_alias FROM apps WHERE id=?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(v, (Some("New Chrome".into()), Some("谷歌浏览器".into())));
    }
    #[test]
    fn crossing_new_year_splits_facts() {
        let c = mem();
        let a = upsert_app(&c, "x.exe", None, None).unwrap();
        let d = NaiveDate::from_ymd_opt(Local::now().year(), 12, 31).unwrap();
        let s = Local
            .from_local_datetime(&d.and_hms_opt(23, 59, 30).unwrap())
            .single()
            .unwrap()
            .timestamp();
        add_duration(&c, a, s, 60).unwrap();
        let dates: Vec<i64> = c
            .prepare("SELECT source_local_date FROM usage_segments ORDER BY source_local_date")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(dates.len(), 2);
        assert_ne!(dates[0] / 10_000, dates[1] / 10_000);
    }
    #[test]
    fn rebuild_restores_caches() {
        let c = mem();
        let a = upsert_app(&c, "x.exe", None, None).unwrap();
        add_duration(&c, a, at(8, 0), 120).unwrap();
        c.execute("DELETE FROM daily_log", []).unwrap();
        c.execute("UPDATE apps SET total_time=0", []).unwrap();
        rebuild_derived(&c).unwrap();
        assert_eq!(
            c.query_row("SELECT time FROM daily_log", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            120
        );
    }

    #[test]
    fn queries_group_only_matching_process_and_alias() {
        let c = mem();
        let local = upsert_app(&c, "chrome.exe", Some("Chrome"), None).unwrap();
        update_app_display_name(&c, local, Some("谷歌浏览器")).unwrap();
        add_duration(&c, local, at(8, 0), 60).unwrap();
        let remote = SegmentRecord {
            origin_device_id: "remote".into(),
            segment_sequence: 1,
            origin_app_id: 1,
            process_name: "CHROME.EXE".into(),
            start_utc: at(9, 0),
            end_utc: at(9, 1),
            source_local_date: today_key(),
            utc_offset_minutes: local_offset_minutes(at(9, 0)),
        };
        import_segments(&c, &[remote]).unwrap();
        c.execute(
            "UPDATE apps SET custom_alias='谷歌浏览器' WHERE origin_device_id='remote'",
            [],
        )
        .unwrap();
        let start = local_midnight_timestamp(Local::now().date_naive());
        let rank = get_app_rank(&c, start, start + SECS_PER_DAY, 10, None).unwrap();
        assert_eq!(rank.len(), 1);
        assert_eq!(rank[0].total_seconds, 120);
        assert_eq!(
            get_app_list(
                &c,
                None,
                None,
                None,
                Some(start),
                Some(start + SECS_PER_DAY),
                false
            )
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            get_stats_range(
                &c,
                BarGranularity::Day,
                start,
                start + SECS_PER_DAY,
                5,
                None
            )
            .unwrap()
            .len(),
            1
        );
    }
}
