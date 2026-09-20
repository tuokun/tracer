import { invoke } from '@tauri-apps/api/core';
import type {
  TodaySummary, CurrentSession, AppRankItem, AppItem,
  CategoryItem, RadarPoint, PieSlice, StatsSummary,
  SyncOverview, SyncRunResult, SyncSetupInput, SyncSetupResult,
} from './types';

export async function getTodaySummary(): Promise<TodaySummary> {
  return invoke('get_today_summary');
}

export async function getDisplayName(): Promise<string> {
  return invoke('get_display_name');
}

export async function getCurrentSession(): Promise<CurrentSession | null> {
  try {
    return await invoke('get_current_session');
  } catch {
    return null;
  }
}

export async function getAppRank(start: number, end: number, limit?: number, deviceId?: string): Promise<AppRankItem[]> {
  return invoke('get_app_rank', { start, end, limit: limit ?? 10, deviceId });
}

export async function getHourlyHeatmap(date: number): Promise<number[]> {
  return invoke('get_hourly_heatmap', { date });
}

export async function getAppList(
  search?: string,
  categoryId?: number,
  sort?: string,
  startTs?: number,
  endTs?: number,
  includeIgnored?: boolean
): Promise<AppItem[]> {
  return invoke('get_app_list', {
    search,
    categoryId,
    sort,
    startTs,
    endTs,
    includeIgnored,
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

export async function getStatsRange(granularity: string, start: number, end: number, limit?: number, deviceId?: string): Promise<[string, number[]][]> {
  return invoke('get_stats_range', { granularity, start, end, limit: limit ?? 5, deviceId });
}

export async function getStatsRadar(start: number, end: number, deviceId?: string): Promise<RadarPoint[]> {
  return invoke('get_stats_radar', { start, end, deviceId });
}

export async function getStatsPie(start: number, end: number, deviceId?: string): Promise<PieSlice[]> {
  return invoke('get_stats_pie', { start, end, deviceId });
}

export async function getStatsSummary(start: number, end: number, deviceId?: string): Promise<StatsSummary> {
  return invoke('get_stats_summary', { start, end, deviceId });
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

export async function revealAppInFolder(executablePath: string): Promise<void> {
  return invoke('reveal_app_in_folder', { executablePath });
}

export async function setAppIgnored(appId: number, ignored: boolean): Promise<void> {
  return invoke('set_app_ignored', { appId, ignored });
}

export async function setCustomAppIcon(
  appId: number, processName: string, data: number[]
): Promise<string> {
  return invoke('set_custom_app_icon', { appId, processName, data });
}

export async function resetCustomAppIcon(appId: number, processName: string): Promise<void> {
  return invoke('reset_custom_app_icon', { appId, processName });
}

export async function notifyFrontendReady(): Promise<void> {
  return invoke('notify_frontend_ready');
}

export interface UpdateCheckResult {
  latest_version: string | null;
  error: string | null;
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  return invoke('check_for_update');
}

export async function configureSync(input: SyncSetupInput): Promise<SyncSetupResult> { return invoke('configure_sync', { input }); }
export async function getSyncOverview(): Promise<SyncOverview> { return invoke('get_sync_overview'); }
export async function syncNow(year: number): Promise<SyncRunResult> { return invoke('sync_now', { year }); }
export async function cancelSync(): Promise<void> { return invoke('cancel_sync'); }
export async function disconnectSync(): Promise<void> { return invoke('disconnect_sync'); }
export async function renameSyncDevice(name: string): Promise<void> { return invoke('rename_sync_device', { name }); }
export async function changeSyncPassword(oldPassword: string | null, newPassword: string, newPasswordConfirm: string): Promise<void> { return invoke('change_sync_password', { oldPassword, newPassword, newPasswordConfirm }); }
export async function resetSyncSpace(newPassword: string, newPasswordConfirm: string, years: number[], confirmed: boolean): Promise<void> { return invoke('reset_sync_space', { newPassword, newPasswordConfirm, years, confirmed }); }
export async function forceResetSyncSpace(input: SyncSetupInput, years: number[], confirmed: boolean): Promise<void> { return invoke('force_reset_sync_space', { input, years, confirmed }); }
export async function rebuildRemoteYear(year: number, confirmed: boolean): Promise<void> { return invoke('rebuild_remote_year', { year, confirmed }); }
export async function restoreLocalYear(year: number, confirmed: boolean): Promise<number> { return invoke('restore_local_year', { year, confirmed }); }
