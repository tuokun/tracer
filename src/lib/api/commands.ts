import { invoke } from '@tauri-apps/api/core';
import type {
  TodaySummary, CurrentSession, AppRankItem, AppItem,
  CategoryItem, RadarPoint, PieSlice, StatsSummary,
} from './types';

export async function getTodaySummary(): Promise<TodaySummary> {
  return invoke('get_today_summary');
}

export async function getCurrentSession(): Promise<CurrentSession | null> {
  try {
    return await invoke('get_current_session');
  } catch {
    return null;
  }
}

export async function getAppRank(start: number, end: number, limit?: number): Promise<AppRankItem[]> {
  return invoke('get_app_rank', { start, end, limit: limit ?? 10 });
}

export async function getHourlyHeatmap(date: number): Promise<number[]> {
  return invoke('get_hourly_heatmap', { date });
}

export async function getAppList(
  search?: string,
  categoryId?: number,
  sort?: string,
  startTs?: number,
  endTs?: number
): Promise<AppItem[]> {
  return invoke('get_app_list', {
    search,
    categoryId,
    sort,
    startTs,
    endTs,
  });
}

export async function getCategories(): Promise<CategoryItem[]> {
  return invoke('get_categories');
}

export async function saveCategory(
  name: string, color?: string, rules?: string, id?: number
): Promise<number> {
  return invoke('save_category', { id, name, color, rules });
}

export async function deleteCategory(id: number): Promise<void> {
  return invoke('delete_category', { id });
}

export async function applyCategoryRules(): Promise<number> {
  return invoke('apply_category_rules');
}

export async function getStatsRange(granularity: string, start: number, end: number, limit?: number): Promise<[string, number[]][]> {
  return invoke('get_stats_range', { granularity, start, end, limit: limit ?? 5 });
}

export async function getStatsRadar(start: number, end: number): Promise<RadarPoint[]> {
  return invoke('get_stats_radar', { start, end });
}

export async function getStatsPie(start: number, end: number): Promise<PieSlice[]> {
  return invoke('get_stats_pie', { start, end });
}

export async function getStatsSummary(start: number, end: number): Promise<StatsSummary> {
  return invoke('get_stats_summary', { start, end });
}

export async function setConfigValue(key: string, value: string): Promise<void> {
  return invoke('set_config_value', { key, value });
}

export async function getAppIcon(
  exePath: string, processName: string
): Promise<string> {
  return invoke('get_app_icon', { exePath, processName });
}

export async function setAppCategory(appId: number, categoryId: number): Promise<void> {
  return invoke('set_app_category', { appId, categoryId });
}

export async function getConfigValue(key: string): Promise<string | null> {
  return invoke('get_config_value', { key });
}

export async function updateAppDisplayName(appId: number, displayName: string | null): Promise<void> {
  return invoke('update_app_display_name', { appId, displayName });
}
