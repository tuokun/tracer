mod core;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

/// 用户是否通过托盘「退出」主动请求退出（区别于关窗触发的 ExitRequested）。
static SHOULD_QUIT: AtomicBool = AtomicBool::new(false);

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 显示主窗口：已存在则聚焦，已被销毁则重建。
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    } else {
        let _ = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
            .title("Tracer")
            .inner_size(800.0, 600.0)
            .build();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(Default::default(), None))
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<core::event::Event>();

            // 数据库。
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = core::db::open(&data_dir.join("tracer.db"))?;
            core::owner::spawn(rx, conn);

            core::tracker::spawn(tx.clone());
            core::power::spawn(tx);

            // 托盘菜单（只保留显示/退出；开机自启放在阶段四前端 UI）。
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("tray-main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Tracer")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        SHOULD_QUIT.store(true, Ordering::SeqCst);
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

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 关窗后保持后台运行：窗口关闭触发 ExitRequested → 阻止退出（留托盘）。
    // 但用户主动点托盘「退出」时放行（SHOULD_QUIT 已在退出按钮回调中置 true）。
    app.run(|_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !SHOULD_QUIT.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
