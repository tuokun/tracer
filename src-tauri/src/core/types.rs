use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct TodaySummary {
    pub total_seconds: i64,
    pub most_used_app: Option<String>,
    pub idle_seconds: i64,
    pub app_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentSession {
    pub process_name: String,
    pub display_name: Option<String>,
    pub start_timestamp: i64,
    pub current_duration: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppRankItem {
    pub process_name: String,
    pub display_name: Option<String>,
    pub icon_path: Option<String>,
    pub executable_path: Option<String>,
    pub total_seconds: i64,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppItem {
    pub id: i64,
    pub process_name: String,
    pub display_name: Option<String>,
    pub executable_path: Option<String>,
    pub icon_path: Option<String>,
    pub total_seconds: i64,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub last_used_date: Option<i64>,
    pub is_ignored: bool,
    pub origin_device_id: String,
    pub device_name: String,
    pub is_current_device: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryItem {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub rules: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadarPoint {
    pub name: String,
    pub value: i64,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PieSlice {
    pub name: String,
    pub value: i64,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsSummary {
    pub total_seconds: i64,
    pub most_active_category: Option<String>,
    pub most_active_app: Option<String>,
    pub daily_average: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncSetupInput {
    pub endpoint: String,
    pub username: String,
    pub webdav_password: String,
    pub directory: Option<String>,
    pub sync_password: String,
    pub sync_password_confirm: String,
    pub device_name: String,
    pub allow_insecure_http: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSetupResult {
    pub joined_existing: bool,
    pub insecure_http: bool,
    pub concurrency_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceItem {
    pub device_id: String,
    pub display_name: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncHistoryItem {
    pub created_at: i64,
    pub year: i32,
    pub success: bool,
    pub uploaded_bytes: i64,
    pub downloaded_bytes: i64,
    pub imported_segments: i64,
    pub duration_ms: i64,
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncOverview {
    pub configured: bool,
    pub insecure_http: bool,
    pub concurrency_mode: Option<String>,
    pub current_device_id: String,
    pub current_device_name: String,
    pub devices: Vec<DeviceItem>,
    pub years: Vec<i32>,
    pub last_success_at: Option<i64>,
    pub history: Vec<SyncHistoryItem>,
    pub status: crate::core::sync::service::SyncStatus,
}
