<script lang="ts">
  import { onMount } from 'svelte';
  import Chart from '$lib/components/Chart.svelte';
  import {
    getStatsRange, getStatsRadar, getStatsPie, getAppRank, getAppIcon
  } from '$lib/api/commands';
  import { formatDuration, todayTimestamp, startOfDay } from '$lib/utils/time';
  import { appDisplayName } from '$lib/utils/format';
  import { CATEGORY_COLORS } from '$lib/utils/colors';
  import { buildBarOption, buildRadarOption, buildPieOption } from '$lib/charts/options';
  import type { RadarPoint, PieSlice, AppRankItem } from '$lib/api/types';

  type Granularity = 'day' | 'week' | 'month' | 'year';

  let granularity = $state<Granularity>('day');
  let cursorTs = $state(todayTimestamp());
  let chartType = $state<'radar' | 'pie'>('radar');

  let barData = $state<[string, number[]][]>([]);
  let barLabels = $state<string[]>([]);
  let radarPoints = $state<RadarPoint[]>([]);
  let pieSlices = $state<PieSlice[]>([]);
  let topApps = $state<AppRankItem[]>([]);
  let iconCache = $state(new Map<string, string>());

  let barOptions = $derived(buildBarOption(barData, barLabels));
  let radarOptions = $derived(buildRadarOption(radarPoints));
  let pieOptions = $derived(buildPieOption(pieSlices));

  onMount(() => loadData());

  function buildLabels(g: string, startTs: number, endTs: number): string[] {
    if (g === 'day') return Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0'));
    if (g === 'week') return ['一', '二', '三', '四', '五', '六', '日'].map(s => `周${s}`);
    if (g === 'month') {
      const days = Math.round((endTs - startTs) / 86400);
      return Array.from({ length: days }, (_, i) => String(i + 1));
    }
    return Array.from({ length: 12 }, (_, i) => `${i + 1}月`);
  }

  async function loadData() {
    const { start, end } = rangeTs();
    barLabels = buildLabels(granularity, start, end);
    [barData, radarPoints, pieSlices, topApps] = await Promise.all([
      getStatsRange(granularity, start, end, 5),
      getStatsRadar(start, end),
      getStatsPie(start, end),
      getAppRank(start, end, 5),
    ]);
    await loadRankIcons();
  }

  async function loadRankIcons() {
    const entries = topApps.filter(a => a.executable_path && !iconCache.has(a.process_name));
    if (!entries.length) return;
    const results = await Promise.allSettled(
      entries.map(a => getAppIcon(a.executable_path!, a.process_name))
    );
    let changed = false;
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') {
        iconCache.set(entries[i].process_name, r.value);
        changed = true;
      }
    });
    if (changed) iconCache = new Map(iconCache);
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

  <div class="top-row-single">
    <div class="card p-3">
      <div class="panel-title">24h 活动分布</div>
      <Chart options={barOptions} height={240} class="mt-2" />
    </div>
  </div>

  <div class="bottom-row">
    <!-- 分类统计（雷达图 / 饼图合二为一，点击切换） -->
    <div class="card p-3" style="display: flex; flex-direction: column;">
      <div class="panel-header-row">
        <div class="panel-title">分类统计</div>
        <div class="chart-switch">
          <button class="switch-btn" class:active={chartType === 'radar'} onclick={() => chartType = 'radar'}>雷达图</button>
          <button class="switch-btn" class:active={chartType === 'pie'} onclick={() => chartType = 'pie'}>饼图</button>
        </div>
      </div>

      {#if chartType === 'radar'}
        <div class="chart-container mt-2">
          <Chart options={radarOptions} height={200} />
          {#if radarPoints.length === 0}
            <div class="text-empty-center" style="margin-top: 80px;">暂无数据</div>
          {/if}
        </div>
      {:else}
        <div class="chart-container mt-2">
          <Chart options={pieOptions} height={160} />
          <div class="pie-legend">
            {#each pieSlices as slice}
              <div class="pie-legend-item">
                <span class="pie-legend-dot" style="background:{slice.color ?? '#9A92C8'}"></span>
                <span class="pie-legend-name">{slice.name}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- 应用排行移动至下方右侧 -->
    <div class="card p-3">
      <div class="panel-title">应用排行</div>
      <div class="rank-list mt-2">
        {#each topApps as app, i}
          <div class="rank-item">
            <span class="rank-num">{i + 1}</span>
            {#if iconCache.get(app.process_name)}
              <img src={iconCache.get(app.process_name)} alt="" class="rank-icon-img" />
            {:else}
              <span class="rank-icon-img rank-icon-placeholder">{app.process_name[0].toUpperCase()}</span>
            {/if}
            <div class="rank-info">
              <span class="rank-name">{appDisplayName(app.display_name, app.process_name)}</span>
              <div class="rank-bar-track">
                <div class="rank-bar-fill" style="width:{app.percentage * 100}%;background:{CATEGORY_COLORS[i % CATEGORY_COLORS.length]}"></div>
              </div>
            </div>
            <span class="rank-time">{formatDuration(app.total_seconds)}</span>
          </div>
        {/each}
        {#if topApps.length === 0}
          <div class="text-empty-center mt-4">暂无数据</div>
        {/if}
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
    color: theme('colors.text.primary');
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
    color: theme('colors.text.secondary');
    padding: 0.25rem 0.75rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    background: #FFFFFF;
    cursor: pointer;
  }

  .gran-btn:hover { background: theme('colors.primary.hover'); color: theme('colors.primary.DEFAULT'); }
  .gran-btn.active { background: theme('colors.primary.DEFAULT'); color: #FFFFFF; border-color: theme('colors.primary.DEFAULT'); }

  .time-nav { display: flex; align-items: center; gap: 0.5rem; }

  .nav-btn {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    background: #FFFFFF;
    cursor: pointer;
  }

  .nav-btn:hover { background: theme('colors.primary.hover'); color: theme('colors.primary.DEFAULT'); }

  .time-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
    min-width: 7em;
    text-align: center;
  }

  .top-row-single { width: 100%; margin-bottom: 1rem; }
  .bottom-row { display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 5fr); gap: 1rem; }

  .p-3 { padding: 0.75rem; }
  .mt-2 { margin-top: 0.5rem; }
  .pt-2 { padding-top: 0.5rem; }

  .panel-title { font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif; font-weight: 600; font-size: 0.5rem; color: theme('colors.text.primary'); }

  /* 顶部切换排版与 Segmented 切换控件 */
  .panel-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
  }

  .chart-switch {
    display: flex;
    background: theme('colors.heatmap.bg');
    padding: 2px;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
  }

  .switch-btn {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.42rem;
    font-weight: 500;
    color: theme('colors.text.secondary');
    padding: 0.15rem 0.5rem;
    border-radius: 3px;
    border: none;
    background: transparent;
    cursor: pointer;
    transition: all 0.2s;
  }

  .switch-btn:hover {
    color: theme('colors.primary.DEFAULT');
  }

  .switch-btn.active {
    background: #FFFFFF;
    color: theme('colors.primary.DEFAULT');
    box-shadow: 0 1px 3px rgba(0,0,0,0.08);
    font-weight: 600;
  }

  .chart-container {
    position: relative;
    width: 100%;
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .text-empty-center {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.tertiary');
    text-align: center;
    padding: 0.5rem 0;
  }

  /* 排行榜组件样式优化 */
  .rank-list { display: flex; flex-direction: column; gap: 0.5rem; }

  .rank-item { display: flex; align-items: center; gap: 0.375rem; }

  .rank-num {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    font-weight: 700;
    color: theme('colors.text.secondary');
    width: 10px;
    text-align: left;
    flex-shrink: 0;
  }
  .rank-item:nth-child(1) .rank-num { color: #F5A623; }
  .rank-item:nth-child(2) .rank-num { color: theme('colors.text.primary'); }
  .rank-item:nth-child(3) .rank-num { color: theme('colors.text.primary'); }

  .rank-icon-img { width: 20px; height: 20px; border-radius: 3px; flex-shrink: 0; }
  .rank-icon-placeholder { display: inline-flex; align-items: center; justify-content: center; background: theme('colors.border'); font-size: 0.4rem; color: theme('colors.text.tertiary'); }

  /* 移除 max-width 并使用 flex: 1 撑满剩余宽度，使时间文字靠最右对齐 */
  .rank-info { flex: 1; display: flex; flex-direction: column; gap: 0.125rem; min-width: 0; }

  .rank-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.primary');
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rank-bar-track { height: 3px; background: theme('colors.heatmap.bg'); border-radius: 2px; overflow: hidden; }
  .rank-bar-fill { height: 100%; background: theme('colors.primary.DEFAULT'); border-radius: 2px; transition: width 0.3s; }

  .rank-time {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.primary');
    flex-shrink: 0;
    min-width: 56px;
    white-space: nowrap;
    text-align: right;
  }

  .summary-strip { display: flex; flex-direction: column; gap: 0.375rem; }

  .summary-item { display: flex; justify-content: space-between; align-items: center; }

  .summary-val {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
  }

  .summary-lbl {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: theme('colors.text.tertiary');
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
    color: theme('colors.text.secondary');
  }
</style>
