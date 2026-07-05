<script lang="ts">
  import { onMount } from 'svelte';
  import { getAppList, getAppIcon } from '$lib/api/commands';
  import { formatDuration } from '$lib/utils/time';
  import type { AppItem } from '$lib/api/types';

  let apps = $state<AppItem[]>([]);
  let search = $state('');
  let sort = $state('time');
  let iconCache = $state(new Map<string, string>());

  onMount(() => loadApps());

  async function loadApps() {
    apps = await getAppList(search || undefined, undefined, sort);
    // 并行加载图标
    const entries = apps.filter(a => a.executable_path && !iconCache.has(a.process_name));
    const results = await Promise.allSettled(
      entries.map(a => getAppIcon(a.executable_path!, a.process_name))
    );
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') {
        iconCache.set(entries[i].process_name, r.value);
      }
    });
  }

  function onSearch(e: Event) {
    search = (e.target as HTMLInputElement).value;
    loadApps();
  }
</script>

<div class="page">
  <h1 class="page-title">应用列表</h1>
  <p class="page-desc">所有已追踪的应用</p>

  <div class="toolbar">
    <input class="search-input" type="text" placeholder="搜索应用..." value={search} oninput={onSearch} />
    <select class="filter-select" bind:value={sort} onchange={loadApps}>
      <option value="time">按时长</option>
      <option value="name">按名称</option>
    </select>
  </div>

  <div class="card">
    <div class="table-header">
      <span class="col-icon"></span>
      <span class="col-name">应用</span>
      <span class="col-path">路径</span>
      <span class="col-total">总时长</span>
      <span class="col-category">分类</span>
    </div>
    {#if apps.length === 0}
      <div class="table-empty">
        <span style="font-family:'Segoe UI',sans-serif;font-size:0.5rem;color:#9A92C8">暂无数据</span>
      </div>
    {:else}
      {#each apps as app}
        <div class="table-row">
          <span class="col-icon">
            {#if iconCache.get(app.process_name)}
              <img src={iconCache.get(app.process_name)} alt="" class="app-icon" />
            {:else}
              <span class="app-icon-placeholder">{app.process_name[0].toUpperCase()}</span>
            {/if}
          </span>
          <span class="col-name">{app.display_name ?? app.process_name}</span>
          <span class="col-path">{app.executable_path ?? '-'}</span>
          <span class="col-total">{formatDuration(app.total_seconds)}</span>
          <span class="col-category">
            {#if app.category_name}
              <span class="cat-pill" style="background:{app.category_color ?? '#E0DCF0'}20;color:{app.category_color ?? '#6A62A0'}">
                {app.category_name}
              </span>
            {:else}
              <span class="text-text-tertiary text-small">—</span>
            {/if}
          </span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .page { max-width: 1000px; }

  .page-title {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 600;
    font-size: 0.85rem;
    color: theme('colors.text.primary');
    margin: 0;
  }

  .page-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
    margin-top: 0.125rem;
    margin-bottom: 1rem;
  }

  .toolbar { display: flex; gap: 0.75rem; margin-bottom: 1rem; }

  .search-input {
    flex: 1;
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    background: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
    outline: none;
  }

  .search-input:focus { border-color: #5048E5; }

  .filter-select {
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    background: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
    outline: none;
  }

  .filter-select:focus { border-color: #5048E5; }

  .table-header {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid theme('colors.border');
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.secondary');
  }

  .table-row {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid theme('colors.heatmap.bg');
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
  }

  .table-row:last-child { border-bottom: none; }

  .col-icon { width: 28px; }
  .col-icon img, .app-icon-placeholder { width: 20px; height: 20px; border-radius: 3px; vertical-align: middle; }
  .app-icon-placeholder { display: inline-flex; align-items: center; justify-content: center; background: theme('colors.border'); font-size: 0.4rem; color: theme('colors.text.tertiary'); }

  .col-name { flex: 2; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .col-path { flex: 3; font-size: 0.4rem; color: theme('colors.text.secondary'); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .col-total { width: 60px; text-align: right; font-family: 'JetBrains Mono', monospace; font-size: 0.5rem; }
  .col-category { width: 60px; text-align: center; }

  .cat-pill {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    border-radius: 4px;
    padding: 0 0.25rem;
    display: inline-block;
  }

  .table-empty { display: flex; align-items: center; justify-content: center; padding: 3rem 0; }
</style>
