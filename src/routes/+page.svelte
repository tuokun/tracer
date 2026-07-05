<script lang="ts">
  import { onMount } from 'svelte';
  import Chart from '$lib/components/Chart.svelte';
  import { getTodaySummary, getCurrentSession, getAppRank, getAppIcon, getHourlyHeatmap, getAppList } from '$lib/api/commands';
  import { formatDuration, todayTimestamp } from '$lib/utils/time';
  import { appDisplayName } from '$lib/utils/format';
  import { CATEGORY_COLORS } from '$lib/utils/colors';
  import { buildHeatOption, buildRingOption } from '$lib/charts/options';
  import type { TodaySummary, CurrentSession, AppRankItem } from '$lib/api/types';

  let summary = $state<TodaySummary | null>(null);
  let session = $state<CurrentSession | null>(null);
  let rank = $state<AppRankItem[]>([]);
  let heatData = $state<number[]>([]);
  let rankIcons = $state(new Map<string, string>());

  let heatOptions = $derived(buildHeatOption(heatData));
  let ringOptions = $derived(buildRingOption(rank));

  onMount(() => {
    loadData();
    const interval = setInterval(async () => {
      session = await getCurrentSession();
      if (session && !rankIcons.has(session.process_name)) {
        await loadSessionIconOnly(session.process_name);
      }
    }, 5000);
    return () => clearInterval(interval);
  });

  async function loadSessionIconOnly(processName: string) {
    try {
      const appItems = await getAppList(processName);
      const matchedApp = appItems.find(a => a.process_name === processName);
      if (matchedApp?.executable_path) {
        const iconBase64 = await getAppIcon(matchedApp.executable_path, processName);
        rankIcons.set(processName, iconBase64);
        rankIcons = new Map(rankIcons);
      }
    } catch (e) {
      console.error("定时加载当前应用图标失败:", e);
    }
  }

  async function loadData() {
    const ts = todayTimestamp();
    [summary, session] = await Promise.all([
      getTodaySummary(),
      getCurrentSession(),
    ]);
    const [fullRank, heatDataVal] = await Promise.all([
      getAppRank(ts, ts + 86400, 20),
      getHourlyHeatmap(ts),
    ]);
    rank = fullRank.slice(0, 5);
    heatData = heatDataVal;

    const appsToLoad = [...fullRank];

    const currentSession = session;
    if (currentSession && !fullRank.some(r => r.process_name === currentSession.process_name)) {
      try {
        const appItems = await getAppList(currentSession.process_name);
        const matchedApp = appItems.find(a => a.process_name === currentSession.process_name);
        if (matchedApp?.executable_path) {
          appsToLoad.push({
            process_name: currentSession.process_name,
            display_name: currentSession.display_name,
            executable_path: matchedApp.executable_path,
            icon_path: matchedApp.icon_path,
            total_seconds: 0,
            category_name: null,
            category_color: null,
            percentage: 0
          });
        }
      } catch (e) {
        console.error("初始化加载当前应用路径失败:", e);
      }
    }

    await loadIcons(appsToLoad);
  }

  async function loadIcons(apps: { process_name: string; executable_path: string | null }[]) {
    const entries = apps.filter(r => r.executable_path && !rankIcons.has(r.process_name));
    if (!entries.length) return;
    const results = await Promise.allSettled(
      entries.map(r => getAppIcon(r.executable_path!, r.process_name))
    );
    let changed = false;
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') {
        rankIcons.set(entries[i].process_name, r.value);
        changed = true;
      }
    });
    if (changed) rankIcons = new Map(rankIcons);
  }

  function currentAppName(): string {
    return session ? appDisplayName(session.display_name, session.process_name) : '—';
  }

  function getMostUsedAppInfo() {
    if (rank.length === 0) return null;
    const topApp = rank[0];
    return {
      name: appDisplayName(topApp.display_name, topApp.process_name),
      processName: topApp.process_name,
      duration: formatDuration(topApp.total_seconds),
      percentage: topApp.percentage
    };
  }

  function formatSessionDuration(seconds: number): string {
    if (seconds < 60) return '刚刚开始';
    const mins = Math.floor(seconds / 60);
    if (mins < 60) return `已专注 ${mins} 分钟`;
    const hrs = Math.floor(mins / 60);
    const remMins = mins % 60;
    if (remMins === 0) return `已专注 ${hrs} 小时`;
    return `已专注 ${hrs} 小时 ${remMins} 分钟`;
  }

  let mostUsedInfo = $derived(getMostUsedAppInfo());
</script>

<div class="page">
  <div class="page-header">
    <h1 class="page-title">今日概览</h1>
    <span class="page-date">
      {new Date().toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit', weekday: 'short' })}
    </span>
  </div>

  <div class="stats-layout">
    <!-- 左侧大数值卡片：今日总时长与应用数 -->
    <div class="main-stat-card card">
      <div class="main-stat-top">
        <span class="lbl-main">今日使用总时长</span>
        <div class="main-stat-icon-wrap">
          <svg xmlns="http://www.w3.org/2000/svg" class="icon-clock" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.8">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>
      </div>
      
      <!-- 主体数字：在除去顶部标签后的剩余高度内居中 -->
      <div class="main-stat-center">
        <span class="num-main">{summary ? formatDuration(summary.total_seconds) : '—'}</span>
      </div>

      <!-- 活跃应用移至右下角绝对定位 -->
      <div class="main-stat-bottom-right">
        <span class="num-sub-val">{summary?.app_count ?? '—'}</span>
        <span class="lbl-sub">个活跃应用</span>
      </div>
    </div>

    <!-- 右侧堆叠条形卡片组 -->
    <div class="right-stack-layout">
      <!-- 最常用应用卡片 -->
      <div class="sub-row-card card">
        <div class="sub-row-left">
          <div class="card-icon-container">
            {#if mostUsedInfo && rankIcons.get(mostUsedInfo.processName)}
              <img src={rankIcons.get(mostUsedInfo.processName)} alt="" class="app-icon-img" />
            {:else}
              <div class="app-icon-placeholder-sub">🏆</div>
            {/if}
          </div>
          <div class="sub-row-details">
            <span class="sub-row-label">最常用应用</span>
            <span class="sub-row-title">{mostUsedInfo ? mostUsedInfo.name : '—'}</span>
          </div>
        </div>
        <div class="sub-row-right">
          <span class="sub-row-value">{mostUsedInfo ? mostUsedInfo.duration : '—'}</span>
          {#if mostUsedInfo && mostUsedInfo.percentage > 0}
            <div class="mini-bar-wrap">
              <div class="mini-bar" style="width: {mostUsedInfo.percentage * 100}%"></div>
            </div>
          {/if}
        </div>
      </div>

      <!-- 当前活跃应用卡片 -->
      <div class="sub-row-card card">
        <div class="sub-row-left">
          <div class="card-icon-container">
            {#if session && rankIcons.get(session.process_name)}
              <img src={rankIcons.get(session.process_name)} alt="" class="app-icon-img" />
            {:else}
              <div class="app-icon-placeholder-sub">🎯</div>
            {/if}
          </div>
          <div class="sub-row-details">
            <div class="flex-align-center gap-1">
              <span class="sub-row-label">当前活跃</span>
              <span class="pulse-dot"></span>
            </div>
            <span class="sub-row-title">{currentAppName()}</span>
          </div>
        </div>
      </div>
    </div>
  </div>

  <div class="grid-2 mt-4">
    <div class="card p-3">
      <div class="panel-title">应用排行</div>
      <div class="rank-list">
        {#each rank as item, i}
          <div class="rank-item">
            <span class="rank-num">{i + 1}</span>
            <div class="rank-icon">
              {#if rankIcons.get(item.process_name)}
                <img src={rankIcons.get(item.process_name)} alt="" class="rank-icon-img" />
              {:else}
                <div class="app-icon-placeholder">{item.process_name[0].toUpperCase()}</div>
              {/if}
            </div>
            <span class="rank-name">{appDisplayName(item.display_name, item.process_name)}</span>
            <div class="rank-bar-wrap">
              <div class="rank-bar" style="width: {item.percentage * 100}%; background: {CATEGORY_COLORS[i % CATEGORY_COLORS.length]}"></div>
            </div>
            <span class="rank-time">{formatDuration(item.total_seconds)}</span>
          </div>
        {/each}
        {#if rank.length === 0}
          <div class="text-empty">暂无数据</div>
        {/if}
      </div>
    </div>
    <div class="card p-3">
      <div class="panel-title">应用分布</div>
      <Chart options={ringOptions} height={200} />
    </div>
  </div>

  <div class="card p-3 mt-4">
    <div class="panel-title">24h 活动热力</div>
    <Chart options={heatOptions} height={100} class="mt-2" />
  </div>
</div>

<style>
  .page { max-width: 900px; }

  .page-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1rem; }
  .page-title { font-family: 'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight: 600; font-size: 0.85rem; color: theme('colors.text.primary'); margin: 0; }
  .page-date { font-family: 'Segoe UI',sans-serif; font-size: 0.5rem; color: theme('colors.text.tertiary'); }

  /* 新的不对称双栏布局 */
  .stats-layout {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 3fr);
    gap: 1rem;
    margin-bottom: 1rem;
    height: 106px;
    width: 100%;
  }

  /* 左侧主大卡片 */
  .main-stat-card {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 0.65rem 0.85rem;
    background: linear-gradient(135deg, rgba(80, 72, 229, 0.05) 0%, rgba(255, 255, 255, 1) 100%);
    border-left: 3.5px solid theme('colors.primary.DEFAULT');
    position: relative;
    overflow: hidden;
    transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .main-stat-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 20px rgba(80, 72, 229, 0.08);
  }

  .main-stat-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    z-index: 2;
  }

  .lbl-main {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    color: theme('colors.text.secondary');
    font-weight: 500;
  }

  /* 全卡物理几何中心绝对居中 */
  .main-stat-center {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 2;
  }

  .num-main {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-size: 1.6rem;
    font-weight: 800;
    color: theme('colors.text.primary');
    letter-spacing: -0.02em;
    line-height: 1;
    text-align: center;
    white-space: nowrap;
  }

  .main-stat-icon-wrap {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: rgba(80, 72, 229, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    color: theme('colors.primary.DEFAULT');
    z-index: 2;
  }

  .icon-clock {
    width: 16px;
    height: 16px;
  }

  /* 右下角定位 */
  .main-stat-bottom-right {
    position: absolute;
    right: 0.85rem;
    bottom: 0.55rem;
    display: flex;
    align-items: baseline;
    gap: 0.15rem;
    z-index: 2;
  }

  .num-sub-val {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-size: 0.75rem;
    font-weight: 700;
    color: theme('colors.text.primary');
  }

  .lbl-sub {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    color: theme('colors.text.tertiary');
  }

  /* 右侧堆叠长条卡片组 */
  .right-stack-layout {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    justify-content: space-between;
  }

  /* 长条形卡片 */
  .sub-row-card {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.4rem 0.7rem;
    background: theme('colors.surface');
    border: 1px solid theme('colors.border');
    transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .sub-row-card:hover {
    transform: translateY(-1.5px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.04);
    border-color: rgba(80, 72, 229, 0.25);
  }

  .sub-row-left {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
    flex: 1;
  }

  .card-icon-container {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    overflow: hidden;
    background: rgba(80, 72, 229, 0.04);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .app-icon-img {
    width: 24px;
    height: 24px;
    object-fit: contain;
  }

  .app-icon-placeholder-sub {
    font-size: 0.55rem;
  }

  .sub-row-details {
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    min-width: 0;
    flex: 1;
  }

  .sub-row-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.42rem;
    color: theme('colors.text.tertiary');
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .sub-row-title {
    font-family: 'Segoe UI', sans-serif;
    font-weight: 600;
    font-size: 0.55rem;
    color: theme('colors.text.primary');
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub-row-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.1rem;
    margin-left: 0.5rem;
    flex-shrink: 0;
  }

  .sub-row-value {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.secondary');
  }

  /* 最常用百分比微型条 */
  .mini-bar-wrap {
    width: 35px;
    height: 3px;
    background: theme('colors.heatmap.bg');
    border-radius: 1.5px;
    overflow: hidden;
  }

  .mini-bar {
    height: 100%;
    background: theme('colors.primary.DEFAULT');
    border-radius: 1.5px;
  }

  /* 呼吸灯绿点 */
  .pulse-dot {
    width: 5px;
    height: 5px;
    background-color: theme('colors.success');
    border-radius: 50%;
    display: inline-block;
    box-shadow: 0 0 0 0 rgba(76, 175, 80, 0.4);
    animation: pulse 1.8s infinite;
  }

  @keyframes pulse {
    0% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(76, 175, 80, 0.7);
    }
    70% {
      transform: scale(1);
      box-shadow: 0 0 0 4px rgba(76, 175, 80, 0);
    }
    100% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(76, 175, 80, 0);
    }
  }

  /* 辅助 Flex 排列工具 */
  .flex-align-center {
    display: flex;
    align-items: center;
  }
  .gap-1 {
    gap: 0.2rem;
  }

  .card { @apply bg-surface border border-border rounded shadow-card; }
  .p-3 { padding: 0.75rem; }
  .mt-2 { margin-top: 0.5rem; }
  .mt-4 { margin-top: 1rem; }

  .panel-title { font-family: 'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight: 600; font-size: 0.5rem; color: theme('colors.text.primary'); }

  .grid-2 { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 3fr); gap: 1rem; width: 100%; box-sizing: border-box; }

  .rank-list { display: flex; flex-direction: column; gap: 0.25rem; margin-top: 0.375rem; }
  .rank-item { display: flex; align-items: center; gap: 0.25rem; }
  .rank-num { font-family: 'JetBrains Mono',monospace; font-size: 0.5rem; font-weight: 700; color: theme('colors.text.secondary'); width: 10px; text-align: left; flex-shrink: 0; }
  .rank-item:nth-child(1) .rank-num { color: #F5A623; }
  .rank-item:nth-child(2) .rank-num { color: theme('colors.text.primary'); }
  .rank-item:nth-child(3) .rank-num { color: theme('colors.text.primary'); }
  .rank-icon { width: 18px; height: 18px; flex-shrink: 0; }
  .rank-icon-img { width: 18px; height: 18px; border-radius: 3px; }
  .app-icon-placeholder { width: 18px; height: 18px; border-radius: 3px; background: theme('colors.border'); display: flex; align-items: center; justify-content: center; font-family: 'Segoe UI',sans-serif; font-size: 0.4rem; color: theme('colors.text.tertiary'); }
  .rank-name { font-family: 'Segoe UI',sans-serif; font-weight: 600; font-size: 0.5rem; color: theme('colors.text.primary'); width: 125px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex-shrink: 0; }
  .rank-bar-wrap { flex: 1; height: 5px; background: theme('colors.heatmap.bg'); border-radius: 3px; overflow: hidden; }
  .rank-bar { height: 100%; background: theme('colors.primary.DEFAULT'); border-radius: 3px; transition: width 0.3s; }
  .rank-time { font-family: 'JetBrains Mono',monospace; font-size: 0.5rem; font-weight: 600; color: theme('colors.text.primary'); width: 50px; text-align: right; flex-shrink: 0; }
  .rank-cat { font-family: 'Segoe UI',sans-serif; font-size: 0.4rem; border-radius: 4px; padding: 0 0.25rem; white-space: nowrap; }
  .text-empty { font-family: 'Segoe UI',sans-serif; font-size: 0.5rem; color: theme('colors.text.tertiary'); text-align: center; padding: 0.5rem 0; }
</style>
