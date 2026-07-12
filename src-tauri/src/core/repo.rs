//! 仓储层 —— 阶段二 P2
//!
//! app 落库与时长累加。核心是 `add_duration`：把一段时间段按**本地小时/天**切分后
//! upsert 进 `hours_log`/`daily_log`，并累加 `apps.total_time`，单事务。
//! 切分逻辑对齐 Tai `Data.UpdateAppDuration`，桶语义见 `方案评审记录.md` 一·1。

use chrono::{Datelike, Local, TimeZone, Timelike};
use rusqlite::{params, Connection};

/// 落库一个 app（按 `process_name` 去重），返回其 id。
///
/// `display_name` / `executable_path` 提供时覆盖旧值，未提供（`None`）时保留旧值——
/// 这样应用升级后 FileDescription 改了会跟着更新；解析失败也不会清空已有数据。
pub fn upsert_app(
    conn: &Connection,
    process_name: &str,
    display_name: Option<&str>,
    executable_path: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO apps(process_name, display_name, executable_path, is_custom_name) VALUES (?1, ?2, ?3, 0) \
         ON CONFLICT(process_name) DO UPDATE SET \
            display_name = CASE WHEN apps.is_custom_name = 1 THEN apps.display_name ELSE COALESCE(excluded.display_name, apps.display_name) END, \
            executable_path = COALESCE(excluded.executable_path, apps.executable_path)",
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
    // 注：date 为"当天零点 unix 时间戳"，与所有 daily_log 查询的语义一致。
    tx.execute(
        "UPDATE apps SET total_time = total_time + ?1 WHERE id = ?2",
        params![duration_secs, app_id],
    )?;
    tx.commit()
}

/// 把段切分为若干本地小时桶：(整点 unix 时间戳, 当天零点 unix 时间戳, 该小时内秒数)。
/// 纯函数，单测覆盖（含跨小时、跨天）。
fn split_hours(start_unix: i64, duration_secs: i64) -> Vec<(i64, i64, i64)> {
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
            // 当天零点 unix 时间戳（与 daily_log 查询语义一致）。
            let day_start = hour_dt
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|d| Local.from_local_datetime(&d).unwrap().timestamp())
                .unwrap_or(bucket_start_ts);
            out.push((bucket_start_ts, day_start, secs));
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

fn upsert_daily(conn: &Connection, app_id: i64, date: i64, secs: i64) -> rusqlite::Result<()> {
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

// ── 查询方法（阶段四前端调用） ──────────────────────────

use crate::core::types::{
    AppItem, AppRankItem, CategoryItem, PieSlice, RadarPoint, StatsSummary, TodaySummary,
};

const SECS_PER_DAY: i64 = 86400;

/// 当天零点的 unix 时间戳（本地时区）。
fn today_start() -> i64 {
    let now = chrono::Local::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|d| chrono::Local.from_local_datetime(&d).unwrap().timestamp())
        .unwrap_or(now.timestamp())
}

/// 今日概要。
pub fn get_today_summary(conn: &Connection) -> rusqlite::Result<TodaySummary> {
    let start = today_start();
    let end = start + SECS_PER_DAY;
    let total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(time),0) FROM daily_log WHERE date >= ?1 AND date < ?2",
            params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let app_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT app_id) FROM daily_log WHERE date >= ?1 AND date < ?2",
            params![start, end],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let most_used: Option<String> = conn
        .query_row(
            "SELECT COALESCE(a.display_name, a.process_name) FROM daily_log d \
             JOIN apps a ON d.app_id = a.id \
             WHERE d.date >= ?1 AND d.date < ?2 \
             ORDER BY d.time DESC LIMIT 1",
            params![start, end],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let now_ts = chrono::Local::now().timestamp();
    let elapsed = (now_ts - start).max(0);
    let idle = (elapsed - total).max(0);
    Ok(TodaySummary {
        total_seconds: total,
        most_used_app: most_used,
        idle_seconds: idle,
        app_count,
    })
}

/// 应用排行（指定范围内总时长，前 N 个）。
pub fn get_app_rank(
    conn: &Connection,
    start_ts: i64,
    end_ts: i64,
    limit: usize,
) -> rusqlite::Result<Vec<AppRankItem>> {
    let total: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(time),0) FROM daily_log WHERE date >= ?1 AND date < ?2",
            params![start_ts, end_ts],
            |r| r.get(0),
        )
        .unwrap_or(0) as f64;
    let mut stmt = conn.prepare(
        "SELECT a.process_name, a.display_name, a.icon_path, a.executable_path, SUM(d.time), \
                c.name, c.color \
         FROM daily_log d \
         JOIN apps a ON d.app_id = a.id \
         LEFT JOIN categories c ON a.category_id = c.id \
         WHERE d.date >= ?1 AND d.date < ?2 \
         GROUP BY a.id \
         ORDER BY SUM(d.time) DESC \
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![start_ts, end_ts, limit as i64], |r| {
        let secs: i64 = r.get(4)?;
        Ok(AppRankItem {
            process_name: r.get(0)?,
            display_name: r.get(1)?,
            icon_path: r.get(2)?,
            executable_path: r.get(3)?,
            total_seconds: secs,
            category_name: r.get(5)?,
            category_color: r.get(6)?,
            percentage: if total > 0.0 { secs as f64 / total } else { 0.0 },
        })
    })?;
    rows.collect()
}

/// 24h 热力图（返回 24 个整点的秒数，索引 0=00:00~00:59, …, 23=23:00~23:59）。
pub fn get_hourly_heatmap(conn: &Connection, date_ts: i64) -> rusqlite::Result<Vec<i64>> {
    let end = date_ts + SECS_PER_DAY;
    let mut stmt = conn.prepare(
        "SELECT h.data_time, SUM(h.time) \
         FROM hours_log h \
         WHERE h.data_time >= ?1 AND h.data_time < ?2 \
         GROUP BY h.data_time",
    )?;
    let rows: Vec<(i64, i64)> = stmt
        .query_map(params![date_ts, end], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    let mut out = vec![0i64; 24];
    for (hour_ts, secs) in rows {
        let hour = ((hour_ts - date_ts) / 3600) as usize;
        if hour < 24 {
            out[hour] = secs;
        }
    }
    Ok(out)
}

/// 应用列表（含搜索/分类过滤/排序/起止时间过滤）。
pub fn get_app_list(
    conn: &Connection,
    search: Option<&str>,
    category_id: Option<i64>,
    sort_by: Option<&str>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> rusqlite::Result<Vec<AppItem>> {
    let mut sql = String::new();
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // 仅当起止时间都给定时按 daily_log 聚合；只传一个退化为累计模式。
    let is_range = matches!((start_ts, end_ts), (Some(_), Some(_)));
    // 排序字段：范围模式按 SUM(d.time) 别名 range_time；累计模式按 apps.total_time。
    let sort_field = match sort_by {
        Some("name") => "a.process_name ASC",
        _ if is_range => "range_time DESC",
        _ => "a.total_time DESC",
    };

    if is_range {
        sql.push_str(
            "SELECT a.id, a.process_name, a.display_name, a.executable_path, \
                    a.icon_path, SUM(d.time) as range_time, c.name, c.color, \
                    MAX(d.date) \
             FROM daily_log d \
             JOIN apps a ON d.app_id = a.id \
             LEFT JOIN categories c ON a.category_id = c.id \
             WHERE d.date >= ? AND d.date < ?"
        );
        params_vec.push(Box::new(start_ts.unwrap()));
        params_vec.push(Box::new(end_ts.unwrap()));
        push_app_filters(&mut sql, &mut params_vec, search, category_id);
        sql.push_str(" GROUP BY a.id");
    } else {
        sql.push_str(
            "SELECT a.id, a.process_name, a.display_name, a.executable_path, \
                    a.icon_path, a.total_time, c.name, c.color, \
                    (SELECT MAX(date) FROM daily_log WHERE app_id = a.id) \
             FROM apps a \
             LEFT JOIN categories c ON a.category_id = c.id \
             WHERE 1=1"
        );
        push_app_filters(&mut sql, &mut params_vec, search, category_id);
    }
    sql.push_str(" ORDER BY ");
    sql.push_str(sort_field);

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |r| {
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
        })
    })?;
    rows.collect()
}

/// 追加应用搜索/分类过滤的 WHERE 片段（`AND (process_name LIKE ? OR display_name LIKE ?)`、
/// `AND a.category_id = ?`）到 SQL 与参数列表。供 `get_app_list` 两种数据源分支共用。
fn push_app_filters(
    sql: &mut String,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    search: Option<&str>,
    category_id: Option<i64>,
) {
    if let Some(s) = search {
        if !s.is_empty() {
            sql.push_str(" AND (a.process_name LIKE ? OR a.display_name LIKE ?)");
            let pat = format!("%{}%", s);
            params.push(Box::new(pat.clone()));
            params.push(Box::new(pat));
        }
    }
    if let Some(cid) = category_id {
        if cid >= 0 {
            sql.push_str(" AND a.category_id = ?");
            params.push(Box::new(cid));
        }
    }
}

/// 全部分类。
pub fn get_categories(conn: &Connection) -> rusqlite::Result<Vec<CategoryItem>> {
    let mut stmt = conn.prepare("SELECT id, name, color, rules FROM categories ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(CategoryItem {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
            rules: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// 保存/更新分类。保存后自动重跑规则匹配。
pub fn save_category(
    conn: &Connection,
    id: Option<i64>,
    name: &str,
    color: Option<&str>,
    rules: Option<&str>,
) -> rusqlite::Result<i64> {
    let result = match id {
        Some(i) => {
            conn.execute(
                "UPDATE categories SET name=?1, color=?2, rules=?3 WHERE id=?4",
                params![name, color, rules, i],
            )?;
            Ok(i)
        }
        None => {
            conn.execute(
                "INSERT INTO categories(name, color, rules) VALUES (?1, ?2, ?3)",
                params![name, color, rules],
            )?;
            Ok(conn.last_insert_rowid())
        }
    };
    if result.is_ok() {
        let _ = apply_category_rules(conn);
    }
    result
}

/// 删除分类。
pub fn delete_category(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE apps SET category_id=0 WHERE category_id=?1", params![id])?;
    conn.execute("DELETE FROM categories WHERE id=?1", params![id])?;
    Ok(())
}

/// 手动指派应用分类。
pub fn set_app_category(conn: &Connection, app_id: i64, category_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE apps SET category_id = ?1 WHERE id = ?2",
        params![category_id, app_id],
    )?;
    Ok(())
}

/// 柱状图粒度。
#[derive(Clone, Copy, Debug)]
pub enum BarGranularity {
    Day,
    Week,
    Month,
    Year,
}

impl BarGranularity {
    /// 字符串 → enum（Tauri 命令从 JS 接收的 granularity）。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }

    /// 该粒度下横轴的桶数。
    pub fn n_buckets(self, start: i64, end: i64) -> usize {
        match self {
            Self::Day => 24,
            Self::Week => 7,
            Self::Month => (((end - start) / SECS_PER_DAY).max(1)) as usize,
            Self::Year => 12,
        }
    }

    /// 给定桶时间戳（hours_log.data_time 或 daily_log.date）+ 范围起点，算桶索引。
    fn bucket_index(self, ts: i64, start: i64) -> usize {
        match self {
            Self::Day => ((ts - start) / 3600) as usize,
            Self::Week | Self::Month => ((ts - start) / SECS_PER_DAY) as usize,
            Self::Year => {
                // 用 chrono 取月份（避免按 30 天估算偏差）。
                let dt = chrono::Local.timestamp_opt(ts, 0).unwrap();
                (dt.month() as usize).saturating_sub(1)
            }
        }
    }
}

/// 柱状图范围聚合：按 `granularity` 分桶，返回每个应用的桶值列表 + "其他"。
///
/// 数据源：
/// - Day → `hours_log`（按小时桶）
/// - Week/Month/Year → `daily_log`（按天或月桶）
///
/// 返回顺序按范围内总时长降序，与 `get_app_rank` 一致（便于配色对齐）。
pub fn get_stats_range(
    conn: &Connection,
    granularity: BarGranularity,
    start: i64,
    end: i64,
    limit: usize,
) -> rusqlite::Result<Vec<(String, Vec<i64>)>> {
    let n = granularity.n_buckets(start, end);
    let sql = match granularity {
        BarGranularity::Day => {
            "SELECT a.id, COALESCE(a.display_name, a.process_name), h.data_time, SUM(h.time) \
             FROM hours_log h \
             JOIN apps a ON h.app_id = a.id \
             WHERE h.data_time >= ?1 AND h.data_time < ?2 \
             GROUP BY a.id, h.data_time"
        }
        _ => {
            "SELECT a.id, COALESCE(a.display_name, a.process_name), d.date, SUM(d.time) \
             FROM daily_log d \
             JOIN apps a ON d.app_id = a.id \
             WHERE d.date >= ?1 AND d.date < ?2 \
             GROUP BY a.id, d.date"
        }
    };
    let mut stmt = conn.prepare(sql)?;
    use std::collections::BTreeMap;
    let mut apps_map: BTreeMap<i64, (String, Vec<i64>, i64)> = BTreeMap::new();
    let rows = stmt.query_map(params![start, end], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    for row in rows.flatten() {
        let (id, name, bucket_ts, secs) = row;
        let idx = granularity.bucket_index(bucket_ts, start);
        if idx >= n {
            continue; // 防御性：超出范围的桶丢弃
        }
        let entry = apps_map.entry(id).or_insert((name, vec![0i64; n], 0));
        entry.1[idx] += secs;
        entry.2 += secs;
    }
    // 按范围内总时长降序，取 Top N，其余合并为"其他"。
    let mut all: Vec<(String, Vec<i64>, i64)> = apps_map.into_values().collect();
    all.sort_by(|a, b| b.2.cmp(&a.2));
    let mut out: Vec<(String, Vec<i64>)> = Vec::new();
    let mut other = vec![0i64; n];
    for (i, (name, vals, _)) in all.into_iter().enumerate() {
        if i < limit {
            out.push((name, vals));
        } else {
            for j in 0..n {
                other[j] += vals[j];
            }
        }
    }
    if out.is_empty() {
        return Ok(vec![]);
    }
    if other.iter().any(|&v| v > 0) {
        out.push(("其他".to_string(), other));
    }
    Ok(out)
}

/// 雷达图数据（分类对比，指定日期范围）。
pub fn get_stats_radar(conn: &Connection, start_ts: i64, end_ts: i64) -> rusqlite::Result<Vec<RadarPoint>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.name,'未分类'), COALESCE(c.color,'#9A92C8'), SUM(d.time) \
         FROM daily_log d \
         JOIN apps a ON d.app_id = a.id \
         LEFT JOIN categories c ON a.category_id = c.id \
         WHERE d.date >= ?1 AND d.date < ?2 \
         GROUP BY COALESCE(c.name,'未分类') \
         ORDER BY 3 DESC",
    )?;
    let rows = stmt.query_map(params![start_ts, end_ts], |r| {
        Ok(RadarPoint {
            name: r.get(0)?,
            value: r.get(2)?,
            color: r.get(1)?,
        })
    })?;
    rows.collect()
}

/// 饼图数据（分类占比，指定日期范围）。
pub fn get_stats_pie(conn: &Connection, start_ts: i64, end_ts: i64) -> rusqlite::Result<Vec<PieSlice>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.name,'未分类'), COALESCE(c.color,'#9A92C8'), SUM(d.time) \
         FROM daily_log d \
         JOIN apps a ON d.app_id = a.id \
         LEFT JOIN categories c ON a.category_id = c.id \
         WHERE d.date >= ?1 AND d.date < ?2 \
         GROUP BY COALESCE(c.name,'未分类') \
         ORDER BY 3 DESC",
    )?;
    let rows = stmt.query_map(params![start_ts, end_ts], |r| {
        Ok(PieSlice {
            name: r.get(0)?,
            value: r.get(2)?,
            color: r.get(1)?,
        })
    })?;
    rows.collect()
}

/// 统计摘要（指定日期范围）。
pub fn get_stats_summary(conn: &Connection, start_ts: i64, end_ts: i64) -> rusqlite::Result<StatsSummary> {
    let total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(time),0) FROM daily_log WHERE date >= ?1 AND date < ?2",
            params![start_ts, end_ts],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let duration_days = ((end_ts - start_ts) as f64 / SECS_PER_DAY as f64).max(1.0);
    let daily_avg = (total as f64 / duration_days) as i64;
    let most_cat: Option<String> = conn
        .query_row(
            "SELECT c.name FROM daily_log d \
             JOIN apps a ON d.app_id = a.id \
             LEFT JOIN categories c ON a.category_id = c.id \
             WHERE d.date >= ?1 AND d.date < ?2 \
             GROUP BY COALESCE(c.name,'未分类') \
             ORDER BY SUM(d.time) DESC LIMIT 1",
            params![start_ts, end_ts],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let most_app: Option<String> = conn
        .query_row(
            "SELECT COALESCE(a.display_name, a.process_name) FROM daily_log d \
             JOIN apps a ON d.app_id = a.id \
             WHERE d.date >= ?1 AND d.date < ?2 \
             GROUP BY a.id ORDER BY SUM(d.time) DESC LIMIT 1",
            params![start_ts, end_ts],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    Ok(StatsSummary {
        total_seconds: total,
        most_active_category: most_cat,
        most_active_app: most_app,
        daily_average: daily_avg,
    })
}

/// 设置配置值。
pub fn set_config(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO config(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 对所有未指定分类（category_id=0）的 app 重跑分类规则。
/// 规则为 glob 通配符，匹配 `process_name` 和 `display_name`。
/// 手动指派的 app（category_id > 0）不会被覆盖。
/// 匹配策略：一个 app 匹配到多个分类时，最后匹配的分类生效（最后匹配者胜出）。
pub fn apply_category_rules(conn: &Connection) -> rusqlite::Result<usize> {
    let cats: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, rules FROM categories WHERE rules IS NOT NULL AND rules != ''")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if cats.is_empty() {
        return Ok(0);
    }
    let mut updated = 0;
    let mut stmt = conn.prepare("SELECT id, process_name, display_name, category_id FROM apps")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?)))?;
    for row in rows {
        let (app_id, pname, dname, cat_id) = match row {
            Ok(r) => r,
            Err(e) => { tracing::warn!("读取 app 行失败: {e}"); continue; }
        };
        if cat_id > 0 {
            continue;
        }
        for (cat_id, rules_str) in &cats {
            for rule in rules_str.split(|c| c == ',' || c == ';' || c == '\n') {
                let rule = rule.trim();
                if rule.is_empty() {
                    continue;
                }
                if glob_match(rule, &pname) || dname.as_deref().is_some_and(|d| glob_match(rule, d)) {
                    conn.execute("UPDATE apps SET category_id = ?1 WHERE id = ?2", params![cat_id, app_id])?;
                    updated += 1;
                    break;
                }
            }
        }
    }
    Ok(updated)
}

/// 简单 glob 通配符匹配（支持 `*` 和 `?`）。
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat = pattern.to_lowercase();
    let name = name.to_lowercase();
    let pat_chars: Vec<char> = pat.chars().collect();
    let name_chars: Vec<char> = name.chars().collect();
    let (np, nn) = (pat_chars.len(), name_chars.len());
    let mut dp = vec![vec![false; nn + 1]; np + 1];
    dp[0][0] = true;
    for i in 1..=np {
        if pat_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=np {
        for j in 1..=nn {
            match pat_chars[i - 1] {
                '*' => dp[i][j] = dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i][j] = dp[i - 1][j - 1],
                c => dp[i][j] = dp[i - 1][j - 1] && c == name_chars[j - 1],
            }
        }
    }
    dp[np][nn]
}

/// 手动重命名应用。如果 `display_name` 是 `Some` 则标记 `is_custom_name` 为 1；
/// 如果为 `None` (清空) 则清除自定义状态并把 `display_name` 设为 `NULL` 以重新恢复自动拉取。
pub fn update_app_display_name(
    conn: &Connection,
    app_id: i64,
    display_name: Option<&str>,
) -> rusqlite::Result<()> {
    if let Some(name) = display_name {
        conn.execute(
            "UPDATE apps SET display_name = ?1, is_custom_name = 1 WHERE id = ?2",
            params![name, app_id],
        )?;
    } else {
        conn.execute(
            "UPDATE apps SET display_name = NULL, is_custom_name = 0 WHERE id = ?1",
            params![app_id],
        )?;
    }
    Ok(())
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

    #[test]
    fn test_app_rename_protection() {
        let conn = mem();
        let app_id = upsert_app(&conn, "test_protect.exe", Some("Auto Name"), None).unwrap();
        
        let app = conn.query_row(
            "SELECT display_name, is_custom_name FROM apps WHERE id = ?1",
            params![app_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        ).unwrap();
        assert_eq!(app.0.as_deref(), Some("Auto Name"));
        assert_eq!(app.1, 0);

        update_app_display_name(&conn, app_id, Some("Manual Name")).unwrap();
        
        let app = conn.query_row(
            "SELECT display_name, is_custom_name FROM apps WHERE id = ?1",
            params![app_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        ).unwrap();
        assert_eq!(app.0.as_deref(), Some("Manual Name"));
        assert_eq!(app.1, 1);

        let new_app_id = upsert_app(&conn, "test_protect.exe", Some("New Auto Name"), None).unwrap();
        assert_eq!(new_app_id, app_id);

        let app = conn.query_row(
            "SELECT display_name, is_custom_name FROM apps WHERE id = ?1",
            params![app_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        ).unwrap();
        assert_eq!(app.0.as_deref(), Some("Manual Name"));
        assert_eq!(app.1, 1);

        update_app_display_name(&conn, app_id, None).unwrap();
        let app = conn.query_row(
            "SELECT display_name, is_custom_name FROM apps WHERE id = ?1",
            params![app_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        ).unwrap();
        assert_eq!(app.0, None);
        assert_eq!(app.1, 0);
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
