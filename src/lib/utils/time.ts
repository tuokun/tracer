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

/** 时间粒度（不含 "全部"，由调用方自行处理）。 */
export type Period = 'day' | 'week' | 'month' | 'year';

/**
 * 按粒度计算 [start, end) 时间戳范围（unix 秒，本地时区）。
 * 前提：`cursorTs` 已对齐到当天零点（用 startOfDay / todayTimestamp 初始化）。
 */
export function rangeForPeriod(period: Period, cursorTs: number): { start: number; end: number } {
  const d = new Date(cursorTs * 1000);
  if (period === 'day') {
    return { start: cursorTs, end: cursorTs + 86400 };
  }
  if (period === 'week') {
    const dow = d.getDay();
    const mon = cursorTs - (dow === 0 ? 6 : dow - 1) * 86400;
    return { start: mon, end: mon + 7 * 86400 };
  }
  if (period === 'month') {
    return {
      start: new Date(d.getFullYear(), d.getMonth(), 1).getTime() / 1000,
      end: new Date(d.getFullYear(), d.getMonth() + 1, 1).getTime() / 1000,
    };
  }
  // year
  return {
    start: new Date(d.getFullYear(), 0, 1).getTime() / 1000,
    end: new Date(d.getFullYear() + 1, 0, 1).getTime() / 1000,
  };
}

/**
 * 按粒度日历步进 cursorTs。
 * 月/年用日历构造（避免固定 30/365 天导致的跨月/跨年偏差）。
 * 返回值仍对齐到当天零点。
 */
export function shiftCursorForPeriod(period: Period, cursorTs: number, delta: number): number {
  const d = new Date(cursorTs * 1000);
  if (period === 'day') return cursorTs + delta * 86400;
  if (period === 'week') return cursorTs + delta * 7 * 86400;
  if (period === 'month') return new Date(d.getFullYear(), d.getMonth() + delta, 1).getTime() / 1000;
  return new Date(d.getFullYear() + delta, 0, 1).getTime() / 1000;
}
