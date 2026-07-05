//! 应用配置 —— 阶段二 P4
//!
//! `config` 表（key-value）启动时载入内存 struct，owner 热路径读内存（评审一·9）。
//! 缺失或非法的键用代码默认值，保证可运行。

use rusqlite::{params, Connection};

const KEY_FLUSH_INTERVAL: &str = "flush_interval_secs";

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// flush 兜底间隔（秒）。idle 判定阈值与之联动（评审一·5：阈值 = 间隔）。
    pub flush_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            flush_interval_secs: 900, // 默认 15 分钟
        }
    }
}

impl Config {
    /// idle 判定阈值（毫秒）= flush 间隔。
    pub fn idle_threshold_ms(&self) -> u64 {
        self.flush_interval_secs * 1000
    }
}

/// 从 `config` 表载入；缺失/非法键回退到默认。
pub fn load(conn: &Connection) -> Config {
    Config {
        flush_interval_secs: read_u64(conn, KEY_FLUSH_INTERVAL)
            .unwrap_or_else(|| Config::default().flush_interval_secs),
    }
}

/// 写入一个 u64 配置（设置项变更时用；阶段四设置 UI 调用）。
#[allow(dead_code)] // 阶段四设置 UI 接入
pub fn set_u64(conn: &Connection, key: &str, value: u64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO config(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value.to_string()],
    )?;
    Ok(())
}

fn read_u64(conn: &Connection, key: &str) -> Option<u64> {
    conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
    .filter(|&v| v > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db;
    use std::path::Path;

    #[test]
    fn defaults_when_empty() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        let cfg = load(&conn);
        assert_eq!(cfg.flush_interval_secs, 900);
        assert_eq!(cfg.idle_threshold_ms(), 900_000);
    }

    #[test]
    fn reads_and_overrides() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        set_u64(&conn, KEY_FLUSH_INTERVAL, 30).unwrap();
        assert_eq!(load(&conn).flush_interval_secs, 30);

        // 非法值回退默认
        conn.execute("UPDATE config SET value='oops' WHERE key=?1", params![KEY_FLUSH_INTERVAL])
            .unwrap();
        assert_eq!(load(&conn).flush_interval_secs, 900);
    }
}
