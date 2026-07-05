mod core;

use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<core::event::Event>();

            // 数据库：放 app_data_dir，必要时建目录。
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = core::db::open(&data_dir.join("tracer.db"))?;
            core::owner::spawn(rx, conn);

            core::tracker::spawn(tx.clone());
            core::power::spawn(tx);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
