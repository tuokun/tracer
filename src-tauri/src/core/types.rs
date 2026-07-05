use serde::Serialize;

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
    pub last_used_date: Option<u32>,
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
