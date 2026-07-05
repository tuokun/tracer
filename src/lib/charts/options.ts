import { formatDuration } from '$lib/utils/time';
import { CATEGORY_COLORS } from '$lib/utils/colors';
import type { AppRankItem, RadarPoint, PieSlice } from '$lib/api/types';

/** 仪表盘 24h 热力柱形图 */
export function buildHeatOption(data: number[]): Record<string, unknown> {
  const hours = Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0'));
  const maxVal = Math.max(...data, 1);
  return {
    grid: { left: 24, right: 4, top: 4, bottom: 20 },
    xAxis: {
      type: 'category', data: hours,
      axisLabel: { fontSize: 8, color: '#9A92C8', interval: 2 },
      axisLine: { show: false }, axisTick: { show: false },
    },
    yAxis: { show: false },
    series: [{
      type: 'bar', barWidth: 14,
      data: data.map(v => v / 60),
      itemStyle: {
        color: (p: { value: number }) => {
          const ratio = p.value / (maxVal / 60);
          if (ratio < 0.2) return 'rgba(80,72,229,0.2)';
          if (ratio < 0.5) return 'rgba(80,72,229,0.5)';
          return '#5048E5';
        },
        borderRadius: [2, 2, 0, 0],
      },
    }],
  };
}

/** 仪表盘分布环形图 */
export function buildRingOption(items: AppRankItem[]): Record<string, unknown> {
  if (!items.length) return {};
  return {
    tooltip: { trigger: 'item', formatter: (p: { name: string; value: number }) => `${p.name}: ${formatDuration(p.value)}` },
    series: [{
      type: 'pie', radius: ['50%', '75%'], center: ['50%', '50%'],
      data: items.map((item, i) => ({
        name: item.display_name ?? item.process_name,
        value: item.total_seconds,
        itemStyle: { color: CATEGORY_COLORS[i % CATEGORY_COLORS.length] },
      })),
      label: { show: false },
      emphasis: { label: { show: false } },
    }],
    graphic: [{
      type: 'text', left: 'center', top: '43%',
      style: { text: `${items.length}`, fill: '#5048E5', font: '600 20px "Segoe UI Variable Display", "Segoe UI", sans-serif', textAlign: 'center' },
    }, {
      type: 'text', left: 'center', top: '56%',
      style: { text: '应用', fill: '#9A92C8', font: '400 8px "Segoe UI", sans-serif', textAlign: 'center' },
    }],
  };
}

/** 统计分析 24h 堆叠柱形图 */
export function buildBarOption(data: [string, number[]][]): Record<string, unknown> {
  const cats = Array.from({ length: 24 }, (_, i) => `${String(i).padStart(2, '0')}:00`);
  return {
    tooltip: { trigger: 'axis' },
    legend: { show: data.length > 1, bottom: 0, textStyle: { fontSize: 9, color: '#6A62A0' } },
    grid: { left: 36, right: 8, top: 8, bottom: data.length > 1 ? 28 : 8 },
    xAxis: { type: 'category', data: cats, axisLabel: { fontSize: 8, color: '#9A92C8' }, axisLine: { show: false }, axisTick: { show: false } },
    yAxis: { type: 'value', splitLine: { lineStyle: { color: '#F0ECF8' } }, axisLabel: { fontSize: 8, color: '#9A92C8' } },
    series: data.length > 0
      ? data.map(([name, vals]) => ({
          name, type: 'bar', stack: 'total',
          data: vals.map(v => Math.round(v / 60)),
          itemStyle: { color: name === '未分类' ? '#9A92C8' : '#5048E5', borderRadius: [1, 1, 0, 0] },
        }))
      : [{ type: 'bar', data: [], itemStyle: { color: '#5048E5' } }],
  };
}

/** 分析页分类雷达图 */
export function buildRadarOption(points: RadarPoint[]): Record<string, unknown> {
  if (!points.length) return {};
  return {
    tooltip: { trigger: 'item' },
    radar: {
      indicator: points.map(p => ({ name: p.name.length > 4 ? p.name.slice(0, 4) : p.name, max: Math.max(...points.map(x => x.value), 1) })),
      axisName: { color: '#6A62A0', fontSize: 9 },
      splitArea: { areaStyle: { color: ['rgba(80,72,229,0.02)', 'rgba(80,72,229,0.04)'] } },
      splitLine: { lineStyle: { color: '#E0DCF0' } },
      axisLine: { lineStyle: { color: '#E0DCF0' } },
    },
    series: [{
      type: 'radar', data: [{ value: points.map(p => p.value), name: '使用时长' }],
      areaStyle: { color: 'rgba(80,72,229,0.15)' },
      lineStyle: { color: '#5048E5', width: 1 },
      itemStyle: { color: '#5048E5' },
    }],
  };
}

/** 分析页分类饼图 */
export function buildPieOption(slices: PieSlice[]): Record<string, unknown> {
  if (!slices.length) return {};
  const total = slices.reduce((a, b) => a + b.value, 0);
  return {
    tooltip: { trigger: 'item', formatter: (p: { name: string; value: number }) => `${p.name}: ${formatDuration(p.value)}` },
    series: [{
      type: 'pie', radius: ['40%', '65%'], center: ['50%', '45%'],
      data: slices.map(s => ({ name: s.name, value: s.value, itemStyle: { color: s.color ?? '#9A92C8' } })),
      label: { show: false },
      emphasis: { label: { show: true, fontSize: 9, fontWeight: 'bold' } },
    }],
    graphic: [{
      type: 'text', left: 'center', top: '38%',
      style: { text: `${Math.round(total / 60)}min`, fill: '#1A1A32', font: '600 11px "Segoe UI Variable Display", "Segoe UI", sans-serif', textAlign: 'center' },
    }],
  };
}
