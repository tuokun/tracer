<script lang="ts">
  import { onMount } from 'svelte';
  import Chart from '$lib/components/Chart.svelte';
  import {
    getStats24h, getStatsRadar, getStatsPie, getStatsSummary, getAppRank
  } from '$lib/api/commands';
  import { formatDuration, todayTimestamp, startOfDay, endOfDay } from '$lib/utils/time';
  import type { RadarPoint, PieSlice, StatsSummary, AppRankItem } from '$lib/api/types';

  type Granularity = 'day' | 'week' | 'month' | 'year';

  let granularity = $state<Granularity>('day');
  let cursorTs = $state(todayTimestamp());

  let barData = $state<[string, number[]][]>([]);
  let radarPoints = $state<RadarPoint[]>([]);
  let pieSlices = $state<PieSlice[]>([]);
  let summary = $state<StatsSummary | null>(null);
  let topApps = $state<AppRankItem[]>([]);

  let barOptions = $derived(buildBarOption(barData));
  let radarOptions = $derived(buildRadarOption(radarPoints));
  let pieOptions = $derived(buildPieOption(pieSlices));

  onMount(() => loadData());

  async function loadData() {
    const { start, end } = rangeTs();
    [barData, radarPoints, pieSlices, summary, topApps] = await Promise.all([
      getStats24h(granularity === 'day' ? cursorTs : start),
      getStatsRadar(start, end),
      getStatsPie(start, end),
      getStatsSummary(start, end),
      getAppRank(granularity === 'day' ? cursorTs : start, 5),
    ]);
  }

  function rangeTs(): { start: number; end: number } {
    const d = new Date(cursorTs * 1000);
    if (granularity === 'day') {
      return { start: cursorTs, end: cursorTs + 86400 };
    }
    if (granularity === 'week') {
      const dow = d.getDay();
      const mon = cursorTs - (dow === 0 ? 6 : dow - 1) * 86400;
      return { start: startOfDay(mon), end: startOfDay(mon) + 7 * 86400 };
    }
    if (granularity === 'month') {
      const start = new Date(d.getFullYear(), d.getMonth(), 1).getTime() / 1000;
      const end = new Date(d.getFullYear(), d.getMonth() + 1, 1).getTime() / 1000;
      return { start: startOfDay(start), end: startOfDay(end) };
    }
    const start = new Date(d.getFullYear(), 0, 1).getTime() / 1000;
    const end = new Date(d.getFullYear() + 1, 0, 1).getTime() / 1000;
    return { start: startOfDay(start), end: startOfDay(end) };
  }

  function setGran(g: Granularity) {
    granularity = g;
    cursorTs = todayTimestamp();
    loadData();
  }

  function nav(delta: number) {
    const factor = granularity === 'day' ? 86400 : granularity === 'week' ? 86400 * 7 : granularity === 'month' ? 86400 * 30 : 86400 * 365;
    cursorTs += delta * factor;
    loadData();
  }

  function rangeLabel(): string {
    const { start, end } = rangeTs();
    const s = new Date(start * 1000);
    const e = new Date((end - 86400) * 1000);
    if (granularity === 'day') return s.toLocaleDateString('zh-CN');
    if (granularity === 'week') return `${s.toLocaleDateString('zh-CN')} - ${e.toLocaleDateString('zh-CN')}`;
    if (granularity === 'month') return `${s.getFullYear()}/${s.getMonth() + 1}`;
    return `${s.getFullYear()}`;
  }

  function buildBarOption(data: [string, number[]][]): Record<string, unknown> {
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

  function buildRadarOption(points: RadarPoint[]): Record<string, unknown> {
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

  function buildPieOption(slices: PieSlice[]): Record<string, unknown> {
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
</script>

<div class="page">
  <div class="page-header">
    <h1 class="page-title">统计分析</h1>
  </div>

  <div class="time-selector">
    <div class="time-granularity">
      <button class="gran-btn" class:active={granularity === 'day'} onclick={() => setGran('day')}>日</button>
      <button class="gran-btn" class:active={granularity === 'week'} onclick={() => setGran('week')}>周</button>
      <button class="gran-btn" class:active={granularity === 'month'} onclick={() => setGran('month')}>月</button>
      <button class="gran-btn" class:active={granularity === 'year'} onclick={() => setGran('year')}>年</button>
    </div>
    <div class="time-nav">
      <button class="nav-btn" onclick={() => nav(-1)}>〈</button>
      <span class="time-label">{rangeLabel()}</span>
      <button class="nav-btn" onclick={() => nav(1)}>〉</button>
    </div>
  </div>

  <div class="top-row">
    <div class="card p-3">
      <div class="panel-title">24h 分布</div>
      <Chart options={barOptions} height={240} class="mt-2" />
    </div>
    <div class="card p-3">
      <div class="panel-title">排行</div>
      <div class="rank-list mt-2">
        {#each topApps as app, i}
          <div class="rank-item">
            <span class="rank-num">{i + 1}</span>
            <span class="rank-name">{app.display_name ?? app.process_name}</span>
            <span class="rank-time">{formatDuration(app.total_seconds)}</span>
          </div>
        {/each}
        {#if topApps.length === 0}
          <div class="text-text-tertiary text-small" style="text-align:center;padding:0.5rem 0">暂无数据</div>
        {/if}
      </div>
      {#if summary}
        <div class="summary-strip mt-2 pt-2" style="border-top:1px solid #E0DCF0">
          <div class="summary-item">
            <span class="summary-val">{formatDuration(summary.total_seconds)}</span>
            <span class="summary-lbl">总时长</span>
          </div>
          <div class="summary-item">
            <span class="summary-val">{summary.most_active_category ?? '-'}</span>
            <span class="summary-lbl">最活跃分类</span>
          </div>
          <div class="summary-item">
            <span class="summary-val">{formatDuration(summary.daily_average)}</span>
            <span class="summary-lbl">日均</span>
          </div>
        </div>
      {/if}
    </div>
  </div>

  <div class="bottom-row">
    <div class="card p-3">
      <div class="panel-title">分类雷达图</div>
      <Chart options={radarOptions} height={240} class="mt-2" />
      {#if radarPoints.length === 0}
        <div class="text-text-tertiary text-small" style="text-align:center;margin-top:-180px">暂无数据</div>
      {/if}
    </div>
    <div class="card p-3">
      <div class="panel-title">分类饼图</div>
      <Chart options={pieOptions} height={240} class="mt-2" />
      <div class="pie-legend">
        {#each pieSlices as slice}
          <div class="pie-legend-item">
            <span class="pie-legend-dot" style="background:{slice.color ?? '#9A92C8'}"></span>
            <span class="pie-legend-name">{slice.name}</span>
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .page { max-width: 1000px; }

  .page-header { margin-bottom: 1rem; }

  .page-title {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 600;
    font-size: 0.85rem;
    color: #1A1A32;
    margin: 0;
  }

  .time-selector {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  .time-granularity { display: flex; gap: 0.25rem; }

  .gran-btn {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: #6A62A0;
    padding: 0.25rem 0.75rem;
    border-radius: 4px;
    border: 1px solid #E0DCF0;
    background: #FFFFFF;
    cursor: pointer;
  }

  .gran-btn:hover { background: rgba(80,72,229,0.08); color: #5048E5; }
  .gran-btn.active { background: #5048E5; color: #FFFFFF; border-color: #5048E5; }

  .time-nav { display: flex; align-items: center; gap: 0.5rem; }

  .nav-btn {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: #6A62A0;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    border: 1px solid #E0DCF0;
    background: #FFFFFF;
    cursor: pointer;
  }

  .nav-btn:hover { background: rgba(80,72,229,0.08); color: #5048E5; }

  .time-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: #1A1A32;
    min-width: 7em;
    text-align: center;
  }

  .top-row { display: grid; gap: 1rem; margin-bottom: 1rem; grid-template-columns: 3fr 1fr; }
  .bottom-row { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }

  .p-3 { padding: 0.75rem; }
  .mt-2 { margin-top: 0.5rem; }
  .pt-2 { padding-top: 0.5rem; }

  .panel-title { font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif; font-weight: 600; font-size: 0.5rem; color: #1A1A32; }

  .rank-list { display: flex; flex-direction: column; gap: 0.375rem; }

  .rank-item { display: flex; align-items: center; gap: 0.375rem; }

  .rank-num {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.4rem;
    color: #9A92C8;
    width: 12px;
    text-align: right;
  }

  .rank-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: #1A1A32;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rank-time {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    color: #6A62A0;
  }

  .summary-strip { display: flex; flex-direction: column; gap: 0.375rem; }

  .summary-item { display: flex; justify-content: space-between; align-items: center; }

  .summary-val {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    color: #1A1A32;
  }

  .summary-lbl {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: #9A92C8;
  }

  .pie-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem 0.75rem;
    margin-top: 0.375rem;
    padding: 0 0.25rem;
  }

  .pie-legend-item { display: flex; align-items: center; gap: 0.25rem; }

  .pie-legend-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .pie-legend-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: #6A62A0;
  }
</style>
