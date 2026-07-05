<script lang="ts">
  import { onMount } from 'svelte';
  import Chart from '$lib/components/Chart.svelte';
  import { getTodaySummary, getCurrentSession, getAppRank, getAppIcon, getHourlyHeatmap } from '$lib/api/commands';
  import { formatDuration, formatTimer, todayTimestamp } from '$lib/utils/time';
  import { buildHeatOption, buildRingOption } from '$lib/charts/options';
  import type { TodaySummary, CurrentSession, AppRankItem } from '$lib/api/types';

  let summary = $state<TodaySummary | null>(null);
  let session = $state<CurrentSession | null>(null);
  let rank = $state<AppRankItem[]>([]);
  let heatData = $state<number[]>([]);
  let timerDisplay = $state('00:00:00');
  let nowLeft = $state(0);
  let rankIcons = $state(new Map<string, string>());

  let timerInterval: ReturnType<typeof setInterval> | undefined;

  let heatOptions = $derived(buildHeatOption(heatData));
  let ringOptions = $derived(buildRingOption(rank));

  onMount(() => {
    loadData();
    updateNowPosition();
    timerInterval = setInterval(() => {
      if (session) {
        const dur = Math.floor(Date.now() / 1000) - session.start_timestamp;
        timerDisplay = formatTimer(dur);
      }
    }, 1000);
    const posInterval = setInterval(updateNowPosition, 60000);
    return () => { clearInterval(timerInterval); clearInterval(posInterval); };
  });

  function updateNowPosition() {
    const now = new Date();
    nowLeft = (now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()) / 864;
  }

  async function loadData() {
    const ts = todayTimestamp();
    summary = await getTodaySummary();
    session = await getCurrentSession();
    if (session) {
      const dur = Math.floor(Date.now() / 1000) - session.start_timestamp;
      timerDisplay = formatTimer(dur);
    }
    [rank, heatData] = await Promise.all([
      getAppRank(ts, 7),
      getHourlyHeatmap(ts),
    ]);
    await loadRankIcons();
  }

  async function loadRankIcons() {
    const entries = rank.filter(r => r.executable_path && !rankIcons.has(r.process_name));
    if (!entries.length) return;
    const results = await Promise.allSettled(
      entries.map(r => getAppIcon(r.executable_path!, r.process_name))
    );
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') {
        rankIcons.set(entries[i].process_name, r.value);
      }
    });
  }

  function sessionStartLabel(): string {
    if (!session) return '--:--';
    const d = new Date(session.start_timestamp * 1000);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  function sessionAppName(): string {
    if (!session) return '—';
    return session.display_name || session.process_name;
  }

</script>

<div class="page">
  <div class="page-header">
    <div>
      <h1 class="page-title">仪表盘</h1>
      <p class="page-desc">今日状态总览</p>
    </div>
    <div class="page-date">
      {new Date().toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' })}
    </div>
  </div>

  <div class="hero-card">
    <div class="hero-left">
      <div class="hero-app-name">{sessionAppName()}</div>
      <div class="hero-since">自 {sessionStartLabel()} 开始使用</div>
    </div>
    <div class="hero-right">
      <div class="hero-timer">{timerDisplay}</div>
      <div class="hero-timer-label">当前段时长</div>
    </div>
  </div>

  <div class="stats-row">
    <div class="stat-card">
      <div class="stat-value">{summary ? formatDuration(summary.total_seconds) : '-'}</div>
      <div class="stat-label">今日使用</div>
    </div>
    <div class="stat-card">
      <div class="stat-value">{summary?.most_used_app ?? '-'}</div>
      <div class="stat-label">最常用</div>
    </div>
    <div class="stat-card">
      <div class="stat-value">{summary ? formatDuration(summary.idle_seconds) : '-'}</div>
      <div class="stat-label">空闲</div>
    </div>
    <div class="stat-card">
      <div class="stat-value">{summary?.app_count ?? '-'}</div>
      <div class="stat-label">应用数</div>
    </div>
  </div>

  <div class="card p-3 mt-4">
    <div class="panel-title">时间轴</div>
    <div class="time-axis">
      <div class="axis-line"></div>
      <div class="axis-labels">
        {#each ['00','03','06','09','12','15','18','21'] as h}
          <span class="axis-label">{h}</span>
        {/each}
      </div>
      <div class="now-marker" style="left: {nowLeft}%">
        <div class="now-dot"></div>
        <div class="now-line"></div>
        <div class="now-label">NOW</div>
      </div>
    </div>
  </div>

  <div class="grid-2 mt-4">
    <div class="card p-3">
      <div class="panel-title">应用分布</div>
      <Chart options={ringOptions} height={200} />
    </div>
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
            <div class="rank-name">{item.display_name ?? item.process_name}</div>
            <div class="rank-bar-wrap">
              <div class="rank-bar" style="width: {item.percentage * 100}%"></div>
            </div>
            <div class="rank-time">{formatDuration(item.total_seconds)}</div>
            {#if item.category_name}
              <div class="rank-cat" style="background:{item.category_color ?? '#E0DCF0'}20;color:{item.category_color ?? '#6A62A0'}">
                {item.category_name}
              </div>
            {/if}
          </div>
        {/each}
        {#if rank.length === 0}
          <div class="text-empty">暂无数据</div>
        {/if}
      </div>
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
  .page-desc { font-family: 'Segoe UI',sans-serif; font-size: 0.5rem; color: theme('colors.text.secondary'); margin-top: 0.125rem; }
  .page-date { font-family: 'Segoe UI',sans-serif; font-size: 0.5rem; color: theme('colors.text.tertiary'); }

  .hero-card { background:theme('colors.sidebar'); border:1px solid theme('colors.border'); border-radius:4px; box-shadow:0 2px 8px rgba(0,0,0,0.05); display:flex; align-items:center; justify-content:space-between; padding:0.75rem; }
  .hero-left { display:flex; flex-direction:column; gap:0.125rem; }
  .hero-app-name { font-family:'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight:600; font-size:1.2rem; color:theme('colors.text.primary'); }
  .hero-since { font-family:'Segoe UI',sans-serif; font-size:0.5rem; color:theme('colors.text.secondary'); }
  .hero-right { display:flex; flex-direction:column; align-items:flex-end; gap:0.125rem; }
  .hero-timer { font-family:'JetBrains Mono',monospace; font-weight:500; font-size:1.2rem; color:theme('colors.primary.DEFAULT'); }
  .hero-timer-label { font-family:'Segoe UI',sans-serif; font-size:0.5rem; color:theme('colors.text.secondary'); }

  .stats-row { display:grid; grid-template-columns:repeat(4,1fr); gap:0.75rem; margin-top:1rem; }
  .stat-card { background:theme('colors.sidebar'); border:1px solid theme('colors.border'); border-radius:4px; box-shadow:0 2px 8px rgba(0,0,0,0.05); display:flex; flex-direction:column; align-items:center; justify-content:center; padding:0.75rem 0.5rem; gap:0.125rem; }
  .stat-value { font-family:'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight:600; font-size:0.75rem; color:theme('colors.text.primary'); }
  .stat-label { font-family:'Segoe UI',sans-serif; font-size:0.5rem; color:theme('colors.text.secondary'); }

  .panel-title { font-family:'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight:600; font-size:0.5rem; color:theme('colors.text.primary'); }

  .time-axis { position:relative; margin-top:0.75rem; height:40px; }
  .axis-line { position:absolute; top:12px; left:0; right:0; height:1px; background:theme('colors.border'); }
  .axis-labels { display:flex; justify-content:space-between; padding:0 0.25rem; position:absolute; top:8px; left:0; right:0; }
  .axis-label { font-family:'JetBrains Mono',monospace; font-size:0.4rem; color:theme('colors.text.tertiary'); }
  .now-marker { position:absolute; top:0; transform:translateX(-50%); display:flex; flex-direction:column; align-items:center; transition:left 60s linear; }
  .now-dot { width:6px; height:6px; border-radius:50%; background:#F5A623; }
  .now-line { width:1px; height:28px; background:#F5A623; }
  .now-label { font-family:'JetBrains Mono',monospace; font-size:0.4rem; color:#F5A623; font-weight:600; margin-top:1px; }

  .grid-2 { display:grid; grid-template-columns:1fr 1fr; gap:1rem; }

  .rank-list { display:flex; flex-direction:column; gap:0.25rem; margin-top:0.375rem; }
  .rank-item { display:flex; align-items:center; gap:0.25rem; }
  .rank-num { font-family:'JetBrains Mono',monospace; font-size:0.4rem; color:theme('colors.text.tertiary'); width:12px; text-align:right; }
  .rank-icon { width:18px; height:18px; flex-shrink:0; }
  .rank-icon-img { width:18px; height:18px; border-radius:3px; }
  .app-icon-placeholder { width:18px; height:18px; border-radius:3px; background:theme('colors.border'); display:flex; align-items:center; justify-content:center; font-family:'Segoe UI',sans-serif; font-size:0.4rem; color:theme('colors.text.tertiary'); }
  .rank-name { font-family:'Segoe UI',sans-serif; font-weight:600; font-size:0.5rem; color:theme('colors.text.primary'); width:55px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .rank-bar-wrap { flex:1; height:5px; background:theme('colors.heatmap.bg'); border-radius:3px; overflow:hidden; }
  .rank-bar { height:100%; background:theme('colors.primary.DEFAULT'); border-radius:3px; transition:width 0.3s; }
  .rank-time { font-family:'JetBrains Mono',monospace; font-size:0.45rem; color:theme('colors.text.secondary'); width:44px; text-align:right; }
  .rank-cat { font-family:'Segoe UI',sans-serif; font-size:0.4rem; border-radius:4px; padding:0 0.25rem; white-space:nowrap; }
  .text-empty { font-family:'Segoe UI',sans-serif; font-size:0.5rem; color:theme('colors.text.tertiary'); text-align:center; padding:0.5rem 0; }
</style>
