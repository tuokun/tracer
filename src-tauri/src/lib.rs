mod core;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
};
use tracing::info;

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

// ── Tauri Commands ──────────────────────────────

#[tauri::command]
fn get_today_summary(state: tauri::State<DbState>) -> Result<TodaySummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_today_summary(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_current_session(
    db: tauri::State<DbState>,
    session: tauri::State<SessionState>,
) -> Result<CurrentSession, String> {
    let current = session.inner.lock().map_err(|e| e.to_string())?.clone();
    let last = session.last_active.lock().map_err(|e| e.to_string())?.clone();
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
                    "SELECT display_name FROM apps WHERE process_name=?1",
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
fn get_app_rank(state: tauri::State<DbState>, start: i64, end: i64, limit: Option<usize>) -> Result<Vec<AppRankItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_app_rank(&conn, start, end, limit.unwrap_or(10)).map_err(|e| e.to_string())
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
) -> Result<Vec<AppItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_app_list(
        &conn,
        search.as_deref(),
        category_id,
        sort.as_deref(),
        start_ts,
        end_ts,
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
    repo::save_category(&conn, id, &name, color.as_deref(), rules.as_deref()).map_err(|e| e.to_string())
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
) -> Result<Vec<(String, Vec<i64>)>, String> {
    let g = core::repo::BarGranularity::from_str(&granularity)
        .ok_or_else(|| format!("未知粒度: {granularity}"))?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_range(&conn, g, start, end, limit.unwrap_or(5)).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_radar(state: tauri::State<DbState>, start: i64, end: i64) -> Result<Vec<RadarPoint>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_radar(&conn, start, end).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_pie(state: tauri::State<DbState>, start: i64, end: i64) -> Result<Vec<PieSlice>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_pie(&conn, start, end).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_stats_summary(state: tauri::State<DbState>, start: i64, end: i64) -> Result<StatsSummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    repo::get_stats_summary(&conn, start, end).map_err(|e| e.to_string())
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
    // 未命中 → 提取 + 编码
    use std::io::Read;
    let path = core::iconer::extract(&exe_path, &state.path, &process_name)
        .ok_or("无关联图标")?;
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
                ).ok()
            });
            if let Some(val) = cfg {
                if let Some((a, b)) = val.split_once(',') {
                    let w = a.trim().parse().unwrap_or(960.0);
                    let h = b.trim().parse().unwrap_or(620.0);
                    if w > 0.0 && h > 0.0 { (w, h) } else { (960.0, 620.0) }
                } else { (960.0, 620.0) }
            } else { (960.0, 620.0) }
        };
        let _ = win.set_size(LogicalSize::new(sw, sh));

        // 还原位置（物理坐标）。
        let pos: Option<(i32, i32)> = db::open(&db_path).ok().and_then(|c| {
            c.query_row(
                "SELECT value FROM config WHERE key = 'window_pos'",
                [],
                |r| r.get::<_, String>(0),
            ).ok()
        }).and_then(|val| {
            let (a, b) = val.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        });
        if let Some((x, y)) = pos.filter(|(x, y)| *x > -30000 && *y > -30000) {
            let _ = win.set_position(PhysicalPosition { x, y });
        }

        let app_clone = app.clone();
        let w_clone = win.clone();
    win.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                if let Ok(size) = w_clone.inner_size() {
                    if let Ok(factor) = w_clone.scale_factor() {
                        let logical = size.to_logical::<f64>(factor);
                        if let Some(state) = app_clone.try_state::<DbState>() {
                            if let Ok(conn) = state.conn.lock() {
                                let _ = repo::set_config(&conn, "window_size", &format!("{},{}", logical.width, logical.height));
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
                            let _ = repo::set_config(&conn, "window_pos", &format!("{},{}", p.x, p.y));
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
                            let _ = repo::set_config(&conn, "window_pos", &format!("{},{}", p.x, p.y));
                        }
                    }
                }
            }
            _ => {}
        }
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
    let Some(state) = app.try_state::<DbState>() else { return light; };
    let Ok(conn) = state.conn.lock() else { return light; };
    let theme: String = conn
        .query_row("SELECT value FROM config WHERE key = 'theme'", [], |r| r.get(0))
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
            .title("Tracer")
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
        .invoke_handler(tauri::generate_handler![
            get_today_summary,
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
            update_app_display_name,
            notify_frontend_ready,
        ])
        .setup(|app| {
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
            app.manage(IconDir { path: icon_dir, cache: Mutex::new(HashMap::new()) });

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
            core::tracker::spawn(tx.clone());
            core::power::spawn(tx);

            // 托盘菜单。
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("tray-main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Tracer")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        SHOULD_QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                        app.exit(0);
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
        .run(|_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !SHOULD_QUIT.load(std::sync::atomic::Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
