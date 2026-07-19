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
