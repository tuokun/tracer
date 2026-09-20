export interface TodaySummary {
  total_seconds: number;
  most_used_app: string | null;
  idle_seconds: number;
  app_count: number;
}

export interface CurrentSession {
  process_name: string;
  display_name: string | null;
  start_timestamp: number;
  current_duration: number;
}

export interface AppRankItem {
  process_name: string;
  display_name: string | null;
  icon_path: string | null;
  executable_path: string | null;
  total_seconds: number;
  category_name: string | null;
  category_color: string | null;
  percentage: number;
}

export interface AppItem {
  id: number;
  process_name: string;
  display_name: string | null;
  executable_path: string | null;
  icon_path: string | null;
  total_seconds: number;
  category_name: string | null;
  category_color: string | null;
  last_used_date: number | null;
  is_ignored: boolean;
  origin_device_id: string;
  device_name: string;
  is_current_device: boolean;
}

export interface CategoryItem {
  id: number;
  name: string;
  color: string | null;
  rules: string | null;
}

export interface RadarPoint {
  name: string;
  value: number;
  color: string | null;
}

export interface PieSlice {
  name: string;
  value: number;
  color: string | null;
}

export interface StatsSummary {
  total_seconds: number;
  most_active_category: string | null;
  most_active_app: string | null;
  daily_average: number;
}

export type SyncPhase = 'idle' | 'checkpoint' | 'download' | 'decrypt' | 'merge' | 'upload' | 'complete' | 'failed' | 'cancelled';
export interface SyncStatus { running: boolean; phase: SyncPhase; year: number | null; message: string | null; }
export interface DeviceItem { device_id: string; display_name: string; is_current: boolean; }
export interface SyncHistoryItem { created_at: number; year: number; success: boolean; uploaded_bytes: number; downloaded_bytes: number; imported_segments: number; duration_ms: number; error_summary: string | null; }
export interface SyncOverview { configured: boolean; insecure_http: boolean; concurrency_mode: string | null; current_device_id: string; current_device_name: string; devices: DeviceItem[]; years: number[]; last_success_at: number | null; history: SyncHistoryItem[]; status: SyncStatus; }
export interface SyncSetupInput { endpoint: string; username: string; webdav_password: string; directory: string | null; sync_password: string; sync_password_confirm: string; device_name: string; allow_insecure_http: boolean; }
export interface SyncSetupResult { joined_existing: boolean; insecure_http: boolean; concurrency_mode: string; }
export interface SyncRunResult { imported_segments: number; downloaded_bytes: number; uploaded_bytes: number; duration_ms: number; }
