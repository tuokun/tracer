//! SQLite 数据库 —— 阶段二 P1
//!
//! 连接管理、WAL 调优、schema 迁移。表结构对齐 `重构方案.md` §二。
//! Connection 归属 owner task（Q4 唯一写者），见 `方案评审记录.md` 一·6。

use std::path::Path;

use rusqlite::Connection;
use tracing::info;

/// 所有未执行迁移（按版本升序）。
/// `schema_version` 表由 `open()` 引导创建，不在此列（否则循环依赖）。
const MIGRATIONS: &[(u32, &str)] = &[
    (1, MIGRATION_V1),
    (2, MIGRATION_V2),
    (3, MIGRATION_V3),
    (4, MIGRATION_V4),
];

// 多端同步采用新的事实模型。按产品决策不迁移旧使用数据，直接重建相关表。
const MIGRATION_V4: &str = r#"
DROP TABLE IF EXISTS hours_log;
DROP TABLE IF EXISTS daily_log;
DROP TABLE IF EXISTS usage_segments;
DROP TABLE IF EXISTS apps;
DROP TABLE IF EXISTS categories;
DROP TABLE IF EXISTS devices;
DROP TABLE IF EXISTS sync_history;

CREATE TABLE devices (
    device_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    metadata_revision INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sync_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    color TEXT,
    rules TEXT,
    logical_revision INTEGER NOT NULL DEFAULT 0,
    revision_device_id TEXT NOT NULL DEFAULT '',
    is_deleted INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    origin_device_id TEXT NOT NULL,
    origin_app_id INTEGER NOT NULL,
    process_name TEXT NOT NULL COLLATE NOCASE,
    system_display_name TEXT,
    custom_alias TEXT,
    executable_path TEXT,
    category_id INTEGER NOT NULL DEFAULT 0,
    icon_path TEXT,
    total_time INTEGER NOT NULL DEFAULT 0,
    is_ignored INTEGER NOT NULL DEFAULT 0,
    custom_icon_path TEXT,
    metadata_revision INTEGER NOT NULL DEFAULT 0,
    UNIQUE(origin_device_id, origin_app_id),
    UNIQUE(origin_device_id, process_name),
    FOREIGN KEY(origin_device_id) REFERENCES devices(device_id)
);
CREATE INDEX idx_apps_process ON apps(process_name);
CREATE INDEX idx_apps_origin ON apps(origin_device_id, origin_app_id);

CREATE TABLE usage_segments (
    origin_device_id TEXT NOT NULL,
    segment_sequence INTEGER NOT NULL,
    app_id INTEGER NOT NULL,
    start_utc INTEGER NOT NULL,
    end_utc INTEGER NOT NULL,
    source_local_date INTEGER NOT NULL,
    utc_offset_minutes INTEGER NOT NULL,
    PRIMARY KEY(origin_device_id, segment_sequence),
    FOREIGN KEY(origin_device_id) REFERENCES devices(device_id),
    FOREIGN KEY(app_id) REFERENCES apps(id)
) WITHOUT ROWID;
CREATE INDEX idx_segments_date ON usage_segments(source_local_date);
CREATE INDEX idx_segments_app ON usage_segments(app_id);

CREATE TABLE hours_log (
    app_id INTEGER NOT NULL,
    source_local_date INTEGER NOT NULL,
    local_hour INTEGER NOT NULL,
    time INTEGER NOT NULL,
    PRIMARY KEY(app_id, source_local_date, local_hour),
    FOREIGN KEY(app_id) REFERENCES apps(id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TABLE daily_log (
    app_id INTEGER NOT NULL,
    source_local_date INTEGER NOT NULL,
    time INTEGER NOT NULL,
    PRIMARY KEY(app_id, source_local_date),
    FOREIGN KEY(app_id) REFERENCES apps(id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TABLE sync_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    year INTEGER NOT NULL,
    success INTEGER NOT NULL,
    uploaded_bytes INTEGER NOT NULL DEFAULT 0,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    imported_segments INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    error_summary TEXT
);
"#;

const MIGRATION_V3: &str = r#"
ALTER TABLE apps ADD COLUMN is_ignored INTEGER NOT NULL DEFAULT 0;
ALTER TABLE apps ADD COLUMN custom_icon_path TEXT;
"#;

const MIGRATION_V2: &str = r#"
ALTER TABLE apps ADD COLUMN is_custom_name INTEGER DEFAULT 0;
"#;

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    process_name TEXT UNIQUE NOT NULL,
    display_name TEXT,
    executable_path TEXT,
    category_id INTEGER DEFAULT 0,
    icon_path TEXT,
    total_time INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_apps_process ON apps(process_name);

CREATE TABLE IF NOT EXISTS hours_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_id INTEGER NOT NULL,
    data_time INTEGER NOT NULL,
    time INTEGER NOT NULL,
    FOREIGN KEY(app_id) REFERENCES apps(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_hours_log_time ON hours_log(data_time);
CREATE INDEX IF NOT EXISTS idx_hours_log_app ON hours_log(app_id);

CREATE TABLE IF NOT EXISTS daily_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_id INTEGER NOT NULL,
    date INTEGER NOT NULL,
    time INTEGER NOT NULL,
    FOREIGN KEY(app_id) REFERENCES apps(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_daily_log_date ON daily_log(date);
CREATE INDEX IF NOT EXISTS idx_daily_log_app ON daily_log(app_id);

CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    color TEXT,
    rules TEXT
);

CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT
);
"#;

/// 打开（必要时创建）数据库，执行 pragma 调优与迁移。
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // 引导版本表（迁移机制依赖它，须先于迁移存在）。
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
        [],
    )?;
    migrate(&conn)?;
    ensure_local_device(&conn)?;
    Ok(conn)
}

/// 返回本机不可变设备 ID；首次打开新 schema 时创建。
pub fn local_device_id(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT value FROM config WHERE key = 'local_device_id'",
        [],
        |r| r.get(0),
    )
}

fn ensure_local_device(conn: &Connection) -> rusqlite::Result<()> {
    let existing = local_device_id(conn).ok();
    let device_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let display_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".to_string());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO config(key, value) VALUES ('local_device_id', ?1)",
        [&device_id],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO devices(device_id, display_name) VALUES (?1, ?2)",
        rusqlite::params![device_id, display_name],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO config(key, value) VALUES ('next_app_sequence', '1')",
        [],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO config(key, value) VALUES ('next_segment_sequence', '1')",
        [],
    )?;
    tx.commit()
}

/// 当前 schema 版本。
pub fn version(conn: &Connection) -> rusqlite::Result<u32> {
    Ok(conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get::<_, u32>(0),
        )
        .unwrap_or(0))
}

/// 用户表数量（启动 sanity 用；排除 `sqlite_` 内部表如 `sqlite_sequence`）。
pub fn table_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |r| r.get(0),
    )
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current = version(conn)?;
    for (v, sql) in MIGRATIONS {
        if *v > current {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("INSERT INTO schema_version(version) VALUES (?1)", [v])?;
            tx.commit()?;
            info!("数据库迁移到 v{v}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_full_schema() {
        let conn = open(Path::new(":memory:")).unwrap();
        assert_eq!(version(&conn).unwrap(), 4);
        assert_eq!(table_count(&conn).unwrap(), 9);
        // 关键表与索引存在
        let has: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name='hours_log' AND type='table'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(has);
    }

    #[test]
    fn migrate_is_idempotent() {
        // 已是 v1 的库再跑 migrate 不应报错、版本不变。
        let conn = open(Path::new(":memory:")).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(version(&conn).unwrap(), 4);
    }

    #[test]
    fn v4_replaces_legacy_usage_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATION_V1).unwrap();
        conn.execute_batch(MIGRATION_V2).unwrap();
        conn.execute(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO schema_version(version) VALUES (1), (2)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO apps(process_name, total_time) VALUES ('demo.exe', 42)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        assert_eq!(version(&conn).unwrap(), 4);
        let old_app_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM apps WHERE process_name='demo.exe')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!old_app_exists);
    }
}
