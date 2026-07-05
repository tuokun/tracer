/** 格式化秒数为 "Xh Ym" 或 "Ym" */
export function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/** 格式化秒数为 "HH:MM:SS" */
export function formatTimer(totalSeconds: number): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

/** 今日零点的 unix 时间戳（秒） */
export function todayTimestamp(): number {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / 1000;
}

/** 指定时间戳所在天的零点（秒） */
export function startOfDay(ts: number): number {
  const d = new Date(ts * 1000);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 1000;
}

/** 指定时间戳所在天的次日零点（秒） */
export function endOfDay(ts: number): number {
  return startOfDay(ts) + 86400;
}
