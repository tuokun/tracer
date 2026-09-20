mod core;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
};
use tracing::info;
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

use core::sync::{
    merge, package,
    service::{self, SyncManager},
    webdav::WebDavClient,
};
use core::types::*;
use core::{db, repo};

/// 通用数据库连接（供 Tauri 命令查询和配置写入使用）。
/// 计时落库由 owner task 独占其写连接，互不冲突（WAL 模式支持一写多读）。
struct DbState {
    conn: Mutex<Connection>,
}

/// 当前前台会话 + 最近的非 tracer 会话（由 owner task 实时更新）。
struct SessionState {
    inner: Arc<Mutex<Option<CurrentSession>>>,
    last_active: Arc<Mutex<Option<CurrentSession>>>,
}

/// 图标存储目录 + 内存缓存。
struct IconDir {
    path: std::path::PathBuf,
    cache: Mutex<HashMap<String, String>>,
}

/// 安装器选择的显示名，启动时读取一次后常驻内存。
struct DisplayName {
    value: &'static str,
}

fn installed_display_name() -> &'static str {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let language = hkcu
        .open_subkey(r"Software\io.github.cgfhsc.tracer")
        .and_then(|key| key.get_value::<String, _>("DisplayLanguage"))
        .ok();

    match language.as_deref() {
        Some("en-US") => "tracer",
        _ => "踪",
    }
}

fn display_name(app: &AppHandle) -> &'static str {
    app.try_state::<DisplayName>()
        .map(|name| name.value)
        .unwrap_or("踪")
}

// ── Tauri Commands ──────────────────────────────

#[tauri::command]
fn get_today_summary(state: tauri::State<DbState>) -> Result<TodaySummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_today_summary(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_display_name(name: tauri::State<DisplayName>) -> String {
    name.value.to_string()
}

#[tauri::command]
fn get_current_session(
    db: tauri::State<DbState>,
    session: tauri::State<SessionState>,
) -> Result<CurrentSession, String> {
    let current = session.inner.lock().map_err(|e| e.to_string())?.clone();
    let last = session
        .last_active
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let mut sess = current.ok_or_else(|| "暂无前台会话".to_string())?;
    sess.current_duration = chrono::Local::now().timestamp() - sess.start_timestamp;
    // 是自身 tracer → 用上一次非 tracer 应用替代
    if sess.process_name == "tracer.exe" {
        if let Some(last_sess) = last {
            sess = last_sess;
            sess.current_duration = chrono::Local::now().timestamp() - sess.start_timestamp;
        }
    }
    // 尝试从 DB 补充 display_name
    if sess.display_name.is_none() {
        if let Ok(conn) = db.conn.lock() {
            sess.display_name = conn
                .query_row(
                    "SELECT COALESCE(custom_alias, system_display_name, process_name) FROM apps \
                     WHERE origin_device_id=(SELECT value FROM config WHERE key='local_device_id') AND process_name=?1",
                    rusqlite::params![sess.process_name],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
        }
    }
    Ok(sess)
}

#[tauri::command]
fn get_app_rank(
    state: tauri::State<DbState>,
    start: i64,
    end: i64,
    limit: Option<usize>,
    device_id: Option<String>,
) -> Result<Vec<AppRankItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_app_rank(&conn, start, end, limit.unwrap_or(10), device_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_hourly_heatmap(state: tauri::State<DbState>, date: i64) -> Result<Vec<i64>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_hourly_heatmap(&conn, date).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_app_list(
    state: tauri::State<DbState>,
    search: Option<String>,
    category_id: Option<i64>,
    sort: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
    include_ignored: Option<bool>,
) -> Result<Vec<AppItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_app_list(
        &conn,
        search.as_deref(),
        category_id,
        sort.as_deref(),
        start_ts,
        end_ts,
        include_ignored.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_categories(state: tauri::State<DbState>) -> Result<Vec<CategoryItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_categories(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_category(
    state: tauri::State<DbState>,
    id: Option<i64>,
    name: String,
    color: Option<String>,
    rules: Option<String>,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::save_category(&conn, id, &name, color.as_deref(), rules.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_category(state: tauri::State<DbState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::delete_category(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_app_category(
    state: tauri::State<DbState>,
    app_id: i64,
    category_id: i64,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::set_app_category(&conn, app_id, category_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_range(
    state: tauri::State<DbState>,
    granularity: String,
    start: i64,
    end: i64,
    limit: Option<usize>,
    device_id: Option<String>,
) -> Result<Vec<(String, Vec<i64>)>, String> {
    let g = core::repo::BarGranularity::from_str(&granularity)
        .ok_or_else(|| format!("未知粒度: {granularity}"))?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_range(
        &conn,
        g,
        start,
        end,
        limit.unwrap_or(5),
        device_id.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_radar(
    state: tauri::State<DbState>,
    start: i64,
    end: i64,
    device_id: Option<String>,
) -> Result<Vec<RadarPoint>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_radar(&conn, start, end, device_id.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_pie(
    state: tauri::State<DbState>,
    start: i64,
    end: i64,
    device_id: Option<String>,
) -> Result<Vec<PieSlice>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_pie(&conn, start, end, device_id.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_summary(
    state: tauri::State<DbState>,
    start: i64,
    end: i64,
    device_id: Option<String>,
) -> Result<StatsSummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_summary(&conn, start, end, device_id.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_category_rules(state: tauri::State<DbState>) -> Result<usize, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::apply_category_rules(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_config_value(
    app: tauri::AppHandle,
    state: tauri::State<DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::set_config(&conn, &key, &value).map_err(|e| e.to_string())?;

    if key == "window_size" {
        if let Some((a, b)) = value.split_once(',') {
            let w: f64 = a.trim().parse().unwrap_or(960.0);
            let h: f64 = b.trim().parse().unwrap_or(620.0);
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_size(LogicalSize::new(w, h));
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn get_app_icon(
    db: tauri::State<DbState>,
    state: tauri::State<IconDir>,
    exe_path: String,
    process_name: String,
) -> Result<String, String> {
    // 查内存缓存
    if let Ok(cache) = state.cache.lock() {
        if let Some(cached) = cache.get(&process_name) {
            if !cached.is_empty() {
                return Ok(cached.clone());
            }
        }
    }
    // 未命中 → 优先读取用户图标，否则自动提取。
    use std::io::Read;
    let custom_path = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        repo::get_app_icon_info(&conn, &process_name)
            .map_err(|e| e.to_string())?
            .1
    };
    let path = match custom_path {
        Some(path) if Path::new(&path).is_file() => path,
        _ => core::iconer::extract(&exe_path, &state.path, &process_name).ok_or("无关联图标")?,
    };
    let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    let data_uri = format!("data:image/png;base64,{b64}");
    // 写入缓存
    if let Ok(mut cache) = state.cache.lock() {
        cache.insert(process_name, data_uri.clone());
    }
    Ok(data_uri)
}

#[tauri::command]
fn reveal_app_in_folder(executable_path: String) -> Result<(), String> {
    let path = Path::new(&executable_path);
    if !path.is_file() {
        return Err("应用文件不存在".to_string());
    }
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map_err(|e| format!("无法打开文件位置: {e}"))?;
    Ok(())
}

/// 检查 GitHub Releases 是否有新版本。仅返回 latest tag，版本比对交给前端。
/// 网络异常不返回 Err，而是 Ok(error=...)，让前端区分"无新版"和"网络异常"。
#[derive(serde::Serialize)]
struct UpdateCheck {
    latest_version: Option<String>,
    error: Option<String>,
}

const RELEASE_API: &str = "https://github.com/tuokun/Tracer/releases.atom";

/// 从 GitHub releases.atom 提取第一个 entry 的 title（= 最新版本号）。
/// 用 Atom feed 而非 REST API，规避 api.github.com 的 IP rate limit。
fn parse_atom_first_tag(feed: &str) -> Option<String> {
    let entry_pos = feed.find("<entry>")?;
    let rest = &feed[entry_pos..];
    let open_start = rest.find("<title>")?;
    let open_end = open_start + "<title>".len();
    let close = rest[open_end..].find("</title>")? + open_end;
    let tag = rest[open_end..close].trim().to_string();
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

#[tauri::command]
async fn check_for_update() -> UpdateCheck {
    let client = match reqwest::Client::builder()
        .user_agent("Tracer/0.2.0 (https://github.com/tuokun/Tracer)")
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return UpdateCheck {
                latest_version: None,
                error: Some(format!("构建 HTTP 客户端失败: {e}")),
            }
        }
    };

    match client.get(RELEASE_API).send().await {
        Ok(res) => {
            let status = res.status();
            if !status.is_success() {
                let body = res.text().await.unwrap_or_default();
                return UpdateCheck {
                    latest_version: None,
                    error: Some(format!("HTTP {status}: {body}")),
                };
            }
            match res.text().await {
                Ok(text) => match parse_atom_first_tag(&text) {
                    Some(tag) => UpdateCheck {
                        latest_version: Some(tag),
                        error: None,
                    },
                    None => UpdateCheck {
                        latest_version: None,
                        error: Some("Atom feed 中找不到版本号".into()),
                    },
                },
                Err(e) => UpdateCheck {
                    latest_version: None,
                    error: Some(format!("读取响应失败: {e}")),
                },
            }
        }
        Err(e) => UpdateCheck {
            latest_version: None,
            error: Some(format!("网络异常: {e}")),
        },
    }
}

#[tauri::command]
fn set_app_ignored(
    state: tauri::State<DbState>,
    session: tauri::State<SessionState>,
    app_id: i64,
    ignored: bool,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::set_app_ignored(&conn, app_id, ignored).map_err(|e| e.to_string())?;
    if ignored {
        let process_name: String = conn
            .query_row(
                "SELECT process_name FROM apps WHERE id = ?1",
                rusqlite::params![app_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if let Ok(mut current) = session.inner.lock() {
            if current
                .as_ref()
                .is_some_and(|s| s.process_name == process_name)
            {
                *current = None;
            }
        }
        if let Ok(mut last) = session.last_active.lock() {
            if last
                .as_ref()
                .is_some_and(|s| s.process_name == process_name)
            {
                *last = None;
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn set_custom_app_icon(
    db: tauri::State<DbState>,
    icons: tauri::State<IconDir>,
    app_id: i64,
    process_name: String,
    data: Vec<u8>,
) -> Result<String, String> {
    if data.is_empty() || data.len() > 10 * 1024 * 1024 {
        return Err("图标文件必须小于 10 MB".to_string());
    }
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let (stored_id, _) =
            repo::get_app_icon_info(&conn, &process_name).map_err(|e| e.to_string())?;
        if stored_id != app_id {
            return Err("应用标识不匹配".to_string());
        }
    }
    let image = image::load_from_memory(&data).map_err(|_| "不支持或损坏的图片文件".to_string())?;
    let image = image.thumbnail(128, 128);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| format!("图标转换失败: {e}"))?;

    let path = icons.path.join(format!("custom-{app_id}.png"));
    std::fs::write(&path, png.get_ref()).map_err(|e| format!("图标保存失败: {e}"))?;
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        repo::set_custom_icon_path(&conn, app_id, Some(&path.to_string_lossy()))
            .map_err(|e| e.to_string())?;
    }
    use base64::Engine;
    let data_uri = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.get_ref())
    );
    if let Ok(mut cache) = icons.cache.lock() {
        cache.insert(process_name, data_uri.clone());
    }
    Ok(data_uri)
}

#[tauri::command]
fn reset_custom_app_icon(
    db: tauri::State<DbState>,
    icons: tauri::State<IconDir>,
    app_id: i64,
    process_name: String,
) -> Result<(), String> {
    let old_path = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let (stored_id, path) =
            repo::get_app_icon_info(&conn, &process_name).map_err(|e| e.to_string())?;
        if stored_id != app_id {
            return Err("应用标识不匹配".to_string());
        }
        repo::set_custom_icon_path(&conn, app_id, None).map_err(|e| e.to_string())?;
        path
    };
    if let Some(path) = old_path {
        let expected = icons.path.join(format!("custom-{app_id}.png"));
        if Path::new(&path) == expected && expected.is_file() {
            std::fs::remove_file(expected).map_err(|e| format!("旧图标删除失败: {e}"))?;
        }
    }
    if let Ok(mut cache) = icons.cache.lock() {
        cache.remove(&process_name);
    }
    Ok(())
}

#[tauri::command]
fn get_config_value(state: tauri::State<DbState>, key: String) -> Result<Option<String>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let val: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    Ok(val)
}

#[tauri::command]
fn update_app_display_name(
    state: tauri::State<DbState>,
    app_id: i64,
    display_name: Option<String>,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::update_app_display_name(&conn, app_id, display_name.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn configure_sync(
    state: tauri::State<'_, DbState>,
    input: SyncSetupInput,
) -> Result<SyncSetupResult, String> {
    if input.sync_password != input.sync_password_confirm {
        return Err("两次输入的同步密码不一致".into());
    }
    if input.sync_password.is_empty() {
        return Err("同步密码不能为空".into());
    }
    let client = WebDavClient::new(
        &input.endpoint,
        input.username.clone(),
        input.webdav_password.clone(),
        input.directory.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    if client.is_insecure() && !input.allow_insecure_http {
        return Err("HTTP WebDAV 会暴露用户名和密码，请确认风险后再保存".into());
    }
    client.ensure_directory().await.map_err(|e| e.to_string())?;
    let concurrency = client
        .detect_concurrency()
        .await
        .map_err(|e| e.to_string())?;
    let remote = client
        .get("manifest.tracer")
        .await
        .map_err(|e| e.to_string())?;
    let (joined, master, mut payload, original) = if let Some(file) = &remote {
        let opened =
            package::open_manifest(&file.bytes, &input.sync_password).map_err(|e| e.to_string())?;
        (
            true,
            opened.master_key,
            opened.payload,
            Some(file.bytes.clone()),
        )
    } else {
        let (bytes, payload, master) =
            package::new_space(&input.sync_password).map_err(|e| e.to_string())?;
        (false, master, payload, Some(bytes))
    };
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        merge::apply_metadata(&conn, &payload.devices, &payload.apps, &payload.categories)
            .map_err(|e| e.to_string())?;
        repo::set_local_device_name(&conn, &input.device_name).map_err(|e| e.to_string())?;
        let local_id = db::local_device_id(&conn).map_err(|e| e.to_string())?;
        let local_devices = repo::list_devices(&conn).map_err(|e| e.to_string())?;
        payload.devices =
            merge::merge_devices(&payload.devices, &local_devices).map_err(|e| e.to_string())?;
        payload.devices.retain(|v| v.device_id != local_id);
        payload.devices.extend(
            local_devices
                .into_iter()
                .filter(|v| v.device_id == local_id),
        );
        let local_apps = repo::export_app_metadata(&conn).map_err(|e| e.to_string())?;
        payload.apps = merge::merge_apps(&payload.apps, &local_apps).map_err(|e| e.to_string())?;
        payload.apps.retain(|v| v.origin_device_id != local_id);
        payload.apps.extend(
            local_apps
                .into_iter()
                .filter(|v| v.origin_device_id == local_id),
        );
        payload.categories = merge::merge_categories(
            &payload.categories,
            &repo::export_categories(&conn).map_err(|e| e.to_string())?,
        );
    }
    let manifest = package::replace_manifest_payload(original.as_ref().unwrap(), &master, &payload)
        .map_err(|e| e.to_string())?;
    client
        .put(
            "manifest.tracer",
            manifest,
            remote.as_ref().and_then(|v| v.etag.as_deref()),
            remote.is_none(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let stored = service::StoredSyncConfig::new(
        input.endpoint,
        input.directory,
        input.username,
        input.webdav_password,
        &master,
        concurrency,
    )
    .map_err(|e| e.to_string())?;
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::save_stored_config(&conn, &stored).map_err(|e| e.to_string())?;
    }
    Ok(SyncSetupResult {
        joined_existing: joined,
        insecure_http: client.is_insecure(),
        concurrency_mode: format!("{:?}", concurrency).to_lowercase(),
    })
}

#[tauri::command]
async fn get_sync_overview(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
) -> Result<SyncOverview, String> {
    let (current, devices, years, history, stored) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let current = db::local_device_id(&conn).map_err(|e| e.to_string())?;
        (
            current,
            repo::list_devices(&conn).map_err(|e| e.to_string())?,
            repo::available_years(&conn).map_err(|e| e.to_string())?,
            repo::sync_history(&conn).map_err(|e| e.to_string())?,
            service::load_stored_config(&conn).map_err(|e| e.to_string())?,
        )
    };
    let current_name = devices
        .iter()
        .find(|v| v.device_id == current)
        .map(|v| v.display_name.clone())
        .unwrap_or_else(|| "Windows PC".into());
    let last_success_at = history.iter().find(|v| v.success).map(|v| v.created_at);
    Ok(SyncOverview {
        configured: stored.is_some(),
        insecure_http: stored
            .as_ref()
            .is_some_and(|v| v.endpoint.starts_with("http://")),
        concurrency_mode: stored
            .as_ref()
            .map(|v| format!("{:?}", v.concurrency).to_lowercase()),
        current_device_id: current.clone(),
        current_device_name: current_name,
        devices: devices
            .into_iter()
            .map(|v| DeviceItem {
                is_current: v.device_id == current,
                device_id: v.device_id,
                display_name: v.display_name,
            })
            .collect(),
        years,
        last_success_at,
        history,
        status: manager.status().await,
    })
}

#[tauri::command]
async fn sync_now(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
    year: i32,
) -> Result<service::SyncRunResult, String> {
    let stored = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::load_stored_config(&conn).map_err(|e| e.to_string())?
    }
    .ok_or("尚未配置 WebDAV 同步")?;
    let (client, master) = stored.client_and_master().map_err(|e| e.to_string())?;
    let result = manager.run(client, master, year, stored.concurrency).await;
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        match &result {
            Ok(v) => {
                let _ = repo::record_sync_history(
                    &conn,
                    year,
                    true,
                    v.uploaded_bytes,
                    v.downloaded_bytes,
                    v.imported_segments,
                    v.duration_ms,
                    None,
                );
            }
            Err(e) => {
                let _ =
                    repo::record_sync_history(&conn, year, false, 0, 0, 0, 0, Some(&e.to_string()));
            }
        }
    }
    result.map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_sync(manager: tauri::State<SyncManager>) {
    manager.cancel();
}
#[tauri::command]
fn disconnect_sync(state: tauri::State<DbState>) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    service::disconnect(&conn).map_err(|e| e.to_string())
}
#[tauri::command]
fn rename_sync_device(state: tauri::State<DbState>, name: String) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::set_local_device_name(&conn, &name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn change_sync_password(
    state: tauri::State<'_, DbState>,
    old_password: Option<String>,
    new_password: String,
    new_password_confirm: String,
) -> Result<(), String> {
    if new_password.is_empty() || new_password != new_password_confirm {
        return Err("新密码不能为空，且两次输入必须一致".into());
    }
    let stored = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::load_stored_config(&conn).map_err(|e| e.to_string())?
    }
    .ok_or("尚未配置 WebDAV 同步")?;
    let (client, master) = stored.client_and_master().map_err(|e| e.to_string())?;
    let remote = client
        .get("manifest.tracer")
        .await
        .map_err(|e| e.to_string())?
        .ok_or("远端控制文件不存在")?;
    let changed = package::change_password(
        &remote.bytes,
        old_password.as_deref(),
        Some(master),
        &new_password,
    )
    .map_err(|e| e.to_string())?;
    client
        .put("manifest.tracer", changed, remote.etag.as_deref(), false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn reset_sync_space(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
    new_password: String,
    new_password_confirm: String,
    years: Vec<i32>,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("重置同步空间需要二次确认".into());
    }
    if new_password.is_empty() || new_password != new_password_confirm {
        return Err("新密码不能为空，且两次输入必须一致".into());
    }
    if manager.is_running().await {
        return Err("同步进行中，不能重置".into());
    }
    let stored = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::load_stored_config(&conn).map_err(|e| e.to_string())?
    }
    .ok_or("尚未配置 WebDAV 同步")?;
    let (client, _) = stored.client_and_master().map_err(|e| e.to_string())?;
    client.ensure_directory().await.map_err(|e| e.to_string())?;
    manager.checkpoint().await.map_err(|e| e.to_string())?;
    for name in client.list_names().await.map_err(|e| e.to_string())? {
        if name == "manifest.tracer" || name.ends_with(".tracer-sync") {
            client.delete(&name).await.map_err(|e| e.to_string())?;
        }
    }
    let (initial, mut payload, master) =
        package::new_space(&new_password).map_err(|e| e.to_string())?;
    let packages = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        payload.devices = repo::list_devices(&conn).map_err(|e| e.to_string())?;
        payload.apps = repo::export_app_metadata(&conn).map_err(|e| e.to_string())?;
        payload.categories = repo::export_categories(&conn).map_err(|e| e.to_string())?;
        let mut built = Vec::new();
        for year in &years {
            let rows = repo::export_segments(&conn, *year).map_err(|e| e.to_string())?;
            let temp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
            let descriptor = package::build_year_package(
                temp.path(),
                &payload.sync_space_id,
                &payload.generation_id,
                *year,
                &uuid::Uuid::new_v4().to_string(),
                &rows,
                &master,
            )
            .map_err(|e| e.to_string())?;
            let bytes = std::fs::read(temp.path()).map_err(|e| e.to_string())?;
            built.push((*year, bytes, descriptor));
        }
        built
    };
    for (year, bytes, descriptor) in packages {
        client
            .put(&format!("{year}.tracer-sync"), bytes, None, true)
            .await
            .map_err(|e| e.to_string())?;
        payload.packages.push(descriptor);
    }
    payload.packages.sort_by_key(|v| v.year);
    let manifest = package::replace_manifest_payload(&initial, &master, &payload)
        .map_err(|e| e.to_string())?;
    client
        .put("manifest.tracer", manifest, None, true)
        .await
        .map_err(|e| e.to_string())?;
    let (username, password) = stored.credentials().map_err(|e| e.to_string())?;
    let updated = service::StoredSyncConfig::new(
        stored.endpoint,
        stored.directory,
        username,
        password,
        &master,
        stored.concurrency,
    )
    .map_err(|e| e.to_string())?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    service::save_stored_config(&conn, &updated).map_err(|e| e.to_string())
}

#[tauri::command]
async fn force_reset_sync_space(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
    input: SyncSetupInput,
    years: Vec<i32>,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("重置同步空间需要二次确认".into());
    }
    if input.sync_password.is_empty() || input.sync_password != input.sync_password_confirm {
        return Err("新密码不能为空，且两次输入必须一致".into());
    }
    if manager.is_running().await {
        return Err("同步进行中，不能重置".into());
    }
    let client = WebDavClient::new(
        &input.endpoint,
        input.username.clone(),
        input.webdav_password.clone(),
        input.directory.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    if client.is_insecure() && !input.allow_insecure_http {
        return Err("HTTP WebDAV 会暴露用户名和密码，请确认风险后再重置".into());
    }
    client.ensure_directory().await.map_err(|e| e.to_string())?;
    let concurrency = client
        .detect_concurrency()
        .await
        .map_err(|e| e.to_string())?;
    manager.checkpoint().await.map_err(|e| e.to_string())?;
    for name in client.list_names().await.map_err(|e| e.to_string())? {
        if name == "manifest.tracer" || name.ends_with(".tracer-sync") {
            client.delete(&name).await.map_err(|e| e.to_string())?;
        }
    }
    let (initial, mut payload, master) =
        package::new_space(&input.sync_password).map_err(|e| e.to_string())?;
    let packages = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        repo::set_local_device_name(&conn, &input.device_name).map_err(|e| e.to_string())?;
        payload.devices = repo::list_devices(&conn).map_err(|e| e.to_string())?;
        payload.apps = repo::export_app_metadata(&conn).map_err(|e| e.to_string())?;
        payload.categories = repo::export_categories(&conn).map_err(|e| e.to_string())?;
        let mut built = Vec::new();
        for year in &years {
            let rows = repo::export_segments(&conn, *year).map_err(|e| e.to_string())?;
            let temp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
            let descriptor = package::build_year_package(
                temp.path(),
                &payload.sync_space_id,
                &payload.generation_id,
                *year,
                &uuid::Uuid::new_v4().to_string(),
                &rows,
                &master,
            )
            .map_err(|e| e.to_string())?;
            built.push((
                *year,
                std::fs::read(temp.path()).map_err(|e| e.to_string())?,
                descriptor,
            ));
        }
        built
    };
    for (year, bytes, descriptor) in packages {
        client
            .put(&format!("{year}.tracer-sync"), bytes, None, true)
            .await
            .map_err(|e| e.to_string())?;
        payload.packages.push(descriptor);
    }
    payload.packages.sort_by_key(|v| v.year);
    let manifest = package::replace_manifest_payload(&initial, &master, &payload)
        .map_err(|e| e.to_string())?;
    client
        .put("manifest.tracer", manifest, None, true)
        .await
        .map_err(|e| e.to_string())?;
    let stored = service::StoredSyncConfig::new(
        input.endpoint,
        input.directory,
        input.username,
        input.webdav_password,
        &master,
        concurrency,
    )
    .map_err(|e| e.to_string())?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    service::save_stored_config(&conn, &stored).map_err(|e| e.to_string())
}

#[tauri::command]
async fn rebuild_remote_year(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
    year: i32,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("重建远端年度包需要二次确认".into());
    }
    if manager.is_running().await {
        return Err("同步进行中，不能重建".into());
    }
    let stored = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::load_stored_config(&conn).map_err(|e| e.to_string())?
    }
    .ok_or("尚未配置 WebDAV 同步")?;
    let (client, master) = stored.client_and_master().map_err(|e| e.to_string())?;
    manager.checkpoint().await.map_err(|e| e.to_string())?;
    let remote = client
        .get("manifest.tracer")
        .await
        .map_err(|e| e.to_string())?
        .ok_or("远端控制文件不存在")?;
    let mut payload =
        package::open_manifest_with_master(&remote.bytes, &master).map_err(|e| e.to_string())?;
    let rows = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        repo::export_segments(&conn, year).map_err(|e| e.to_string())?
    };
    let temp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let descriptor = package::build_year_package(
        temp.path(),
        &payload.sync_space_id,
        &payload.generation_id,
        year,
        &uuid::Uuid::new_v4().to_string(),
        &rows,
        &master,
    )
    .map_err(|e| e.to_string())?;
    client
        .put(
            &format!("{year}.tracer-sync"),
            std::fs::read(temp.path()).map_err(|e| e.to_string())?,
            None,
            false,
        )
        .await
        .map_err(|e| e.to_string())?;
    payload.packages.retain(|v| v.year != year);
    payload.packages.push(descriptor);
    payload.packages.sort_by_key(|v| v.year);
    let manifest = package::replace_manifest_payload(&remote.bytes, &master, &payload)
        .map_err(|e| e.to_string())?;
    client
        .put("manifest.tracer", manifest, remote.etag.as_deref(), false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn restore_local_year(
    state: tauri::State<'_, DbState>,
    manager: tauri::State<'_, SyncManager>,
    year: i32,
    confirmed: bool,
) -> Result<usize, String> {
    if !confirmed {
        return Err("从远端重建本地年度数据需要二次确认".into());
    }
    if manager.is_running().await {
        return Err("同步进行中，不能恢复".into());
    }
    let stored = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        service::load_stored_config(&conn).map_err(|e| e.to_string())?
    }
    .ok_or("尚未配置 WebDAV 同步")?;
    let (client, master) = stored.client_and_master().map_err(|e| e.to_string())?;
    let manifest_file = client
        .get("manifest.tracer")
        .await
        .map_err(|e| e.to_string())?
        .ok_or("远端控制文件不存在")?;
    let manifest = package::open_manifest_with_master(&manifest_file.bytes, &master)
        .map_err(|e| e.to_string())?;
    let descriptor = manifest
        .packages
        .iter()
        .find(|v| v.year == year)
        .ok_or("远端没有所选年份")?;
    let package_file = client
        .get(&format!("{year}.tracer-sync"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("远端年度包不存在")?;
    let temp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    std::fs::write(temp.path(), package_file.bytes).map_err(|e| e.to_string())?;
    let records = package::open_year_package(
        temp.path(),
        descriptor,
        &manifest.sync_space_id,
        &manifest.generation_id,
        &master,
    )
    .map_err(|e| e.to_string())?;
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        merge::apply_metadata(
            &conn,
            &manifest.devices,
            &manifest.apps,
            &manifest.categories,
        )
        .map_err(|e| e.to_string())?;
    }
    manager
        .replace_year(year, records)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn notify_frontend_ready(app: AppHandle) -> Result<(), String> {
    reveal_hidden_main_window(&app);
    Ok(())
}

/// 窗口大小 & 位置还原与监听。
fn restore_and_monitor_window_size(app: &AppHandle, win: &tauri::WebviewWindow) {
    if let Ok(data_dir) = app.path().app_data_dir() {
        let db_path = data_dir.join("tracer.db");
        // 优先级：1) config 表里的 window_size → 2) 960×620 默认
        let (sw, sh): (f64, f64) = {
            // 临时读取 config 表
            let cfg = db::open(&db_path).ok().and_then(|c| {
                c.query_row(
                    "SELECT value FROM config WHERE key = 'window_size'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok()
            });
            if let Some(val) = cfg {
                if let Some((a, b)) = val.split_once(',') {
                    let w = a.trim().parse().unwrap_or(960.0);
                    let h = b.trim().parse().unwrap_or(620.0);
                    if w > 0.0 && h > 0.0 {
                        (w, h)
                    } else {
                        (960.0, 620.0)
                    }
                } else {
                    (960.0, 620.0)
                }
            } else {
                (960.0, 620.0)
            }
        };
        let _ = win.set_size(LogicalSize::new(sw, sh));

        // 还原位置（物理坐标）。
        let pos: Option<(i32, i32)> = db::open(&db_path)
            .ok()
            .and_then(|c| {
                c.query_row(
                    "SELECT value FROM config WHERE key = 'window_pos'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok()
            })
            .and_then(|val| {
                let (a, b) = val.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            });
        if let Some((x, y)) = pos.filter(|(x, y)| *x > -30000 && *y > -30000) {
            let _ = win.set_position(PhysicalPosition { x, y });
        }

        let app_clone = app.clone();
        let w_clone = win.clone();
        win.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                if let Ok(size) = w_clone.inner_size() {
                    if let Ok(factor) = w_clone.scale_factor() {
                        let logical = size.to_logical::<f64>(factor);
                        if let Some(state) = app_clone.try_state::<DbState>() {
                            if let Ok(conn) = state.conn.lock() {
                                let _ = repo::set_config(
                                    &conn,
                                    "window_size",
                                    &format!("{},{}", logical.width, logical.height),
                                );
                            }
                        }
                    }
                }
                if let Ok(p) = w_clone.outer_position() {
                    if p.x <= -30000 || p.y <= -30000 {
                        return;
                    }
                    if let Some(state) = app_clone.try_state::<DbState>() {
                        if let Ok(conn) = state.conn.lock() {
                            let _ =
                                repo::set_config(&conn, "window_pos", &format!("{},{}", p.x, p.y));
                        }
                    }
                }
            }
            tauri::WindowEvent::Moved(_) => {
                if let Ok(p) = w_clone.outer_position() {
                    if p.x <= -30000 || p.y <= -30000 {
                        return;
                    }
                    if let Some(state) = app_clone.try_state::<DbState>() {
                        if let Ok(conn) = state.conn.lock() {
                            let _ =
                                repo::set_config(&conn, "window_pos", &format!("{},{}", p.x, p.y));
                        }
                    }
                }
            }
            _ => {}
        });
    }
}

/// 用户是否主动请求退出（区别于关窗触发的 ExitRequested）。
static SHOULD_QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 读取 Windows 注册表判断系统当前是否为深色主题。
#[cfg(windows)]
fn system_is_dark() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    if let Ok(key) = hkcu.open_subkey(path) {
        if let Ok(val) = key.get_value::<u32, _>("AppsUseLightTheme") {
            return val == 0;
        }
    }
    false
}

#[cfg(not(windows))]
fn system_is_dark() -> bool {
    false
}

/// 根据数据库存储的主题配置，返回与之匹配的窗口原生背景色。
/// 用于在 WebView2 唤醒的初始几百毫秒内避免露出默认白底。
fn theme_window_bg(app: &AppHandle) -> tauri::window::Color {
    let light = tauri::window::Color(240, 240, 236, 255);
    let Some(state) = app.try_state::<DbState>() else {
        return light;
    };
    let Ok(conn) = state.conn.lock() else {
        return light;
    };
    let theme: String = conn
        .query_row("SELECT value FROM config WHERE key = 'theme'", [], |r| {
            r.get(0)
        })
        .unwrap_or_else(|_| "system".to_string());
    match theme.as_str() {
        "pure" => tauri::window::Color(255, 255, 255, 255),
        "dark" => tauri::window::Color(15, 23, 42, 255),
        "ocean" => tauri::window::Color(11, 25, 44, 255),
        "forest" => tauri::window::Color(6, 78, 59, 255),
        "system" => {
            if system_is_dark() {
                tauri::window::Color(15, 23, 42, 255)
            } else {
                light
            }
        }
        _ => light,
    }
}

/// 显示主窗口：原生背景色覆盖 WebView2 恢复期间的默认白底。
fn show_and_focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn reveal_hidden_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if !w.is_visible().unwrap_or(false) {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

fn show_main_window(app: &AppHandle) {
    let bg = theme_window_bg(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_background_color(Some(bg));
        show_and_focus_main_window(app);
    } else {
        if let Ok(win) = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
            .title(display_name(app))
            .inner_size(960.0, 620.0)
            .decorations(false)
            .background_color(bg)
            .visible(false)
            .build()
        {
            restore_and_monitor_window_size(app, &win);
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                reveal_hidden_main_window(&app_clone);
            });
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(Default::default(), None))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .invoke_handler(tauri::generate_handler![
            get_today_summary,
            get_display_name,
            get_current_session,
            get_app_rank,
            get_hourly_heatmap,
            get_app_list,
            get_categories,
            save_category,
            delete_category,
            apply_category_rules,
            set_app_category,
            get_stats_range,
            get_stats_radar,
            get_stats_pie,
            get_stats_summary,
            set_config_value,
            get_config_value,
            get_app_icon,
            reveal_app_in_folder,
            set_app_ignored,
            set_custom_app_icon,
            reset_custom_app_icon,
            update_app_display_name,
            notify_frontend_ready,
            check_for_update,
            configure_sync,
            get_sync_overview,
            sync_now,
            cancel_sync,
            disconnect_sync,
            rename_sync_device,
            change_sync_password,
            reset_sync_space,
            force_reset_sync_space,
            rebuild_remote_year,
            restore_local_year,
        ])
        .setup(|app| {
            let display_name = installed_display_name();
            app.manage(DisplayName {
                value: display_name,
            });
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_title(display_name);
            }

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<core::event::Event>();

            // 数据库（写连接归 owner task）。
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("tracer.db");
            let write_conn = db::open(&db_path)?;

            // 窗口大小还原 & 监听。
            if let Some(win) = app.get_webview_window("main") {
                restore_and_monitor_window_size(app.handle(), &win);
            }

            // 图标缓存目录。
            let icon_dir = data_dir.join("icons");
            std::fs::create_dir_all(&icon_dir)?;
            app.manage(IconDir {
                path: icon_dir,
                cache: Mutex::new(HashMap::new()),
            });

            // 读连接（供 Tauri 命令使用）。
            let read_conn = db::open(&db_path)?;
            app.manage(DbState {
                conn: Mutex::new(read_conn),
            });

            // 当前会话状态（owner task 写，Tauri 命令读）。
            let session_state = Arc::new(Mutex::new(None::<CurrentSession>));
            let last_active = Arc::new(Mutex::new(None::<CurrentSession>));
            app.manage(SessionState {
                inner: session_state.clone(),
                last_active: last_active.clone(),
            });

            core::owner::spawn(rx, write_conn, session_state, last_active);
            app.manage(SyncManager::new(tx.clone(), db_path.clone()));
            core::tracker::spawn(tx.clone());
            core::power::spawn(tx);

            // 托盘菜单。
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("tray-main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(display_name)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        if let Some(manager) = app.try_state::<SyncManager>() {
                            manager.cancel();
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                while app.state::<SyncManager>().is_running().await {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                                SHOULD_QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                                app.exit(0);
                            });
                        } else {
                            SHOULD_QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.exit(0);
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            info!("Tracer 启动完成");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !SHOULD_QUIT.load(std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_exit();
                } else if let Some(manager) = handle.try_state::<SyncManager>() {
                    manager.cancel();
                }
            }
        });
}
