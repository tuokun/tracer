<script lang="ts">
  import { onMount } from 'svelte';
  import { getAppList, getAppIcon, getCategories, setAppCategory } from '$lib/api/commands';
  import { formatDuration, startOfDay, rangeForPeriod, shiftCursorForPeriod } from '$lib/utils/time';
  import type { Period as BasePeriod } from '$lib/utils/time';
  import { appDisplayName } from '$lib/utils/format';
  import type { AppItem, CategoryItem } from '$lib/api/types';

  let apps = $state<AppItem[]>([]);
  let cats = $state<CategoryItem[]>([]);
  let search = $state('');
  let sort = $state('time');
  let iconCache = $state(new Map<string, string>());

  // 时间段与日期游标状态（'all' = 历史累计，不参与范围计算）
  type AppPeriod = BasePeriod | 'all';
  let period = $state<AppPeriod>('day');
  let cursorTs = $state(startOfDay(Date.now() / 1000));

  // 自定义右键菜单状态
  let contextMenu = $state<{
    show: boolean;
    x: number;
    y: number;
    appId: number;
    appName: string;
  }>({ show: false, x: 0, y: 0, appId: 0, appName: '' });

  onMount(async () => {
    cats = await getCategories();
    await loadApps();
  });

  let periodLabel = $derived.by(() => {
    if (period === 'all') return '历史累计全部';
    const d = new Date(cursorTs * 1000);
    if (period === 'day') {
      return d.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' });
    }
    if (period === 'week') {
      const range = rangeForPeriod(period, cursorTs);
      const s = new Date(range.start * 1000);
      const e = new Date((range.end - 1) * 1000);
      return `${s.getMonth() + 1}月${s.getDate()}日 - ${e.getMonth() + 1}月${e.getDate()}日`;
    }
    if (period === 'month') {
      return d.toLocaleDateString('zh-CN', { year: 'numeric', month: 'long' });
    }
    return `${d.getFullYear()}年`;
  });

  async function loadApps() {
    const range = period === 'all' ? undefined : rangeForPeriod(period, cursorTs);
    apps = await getAppList(search || undefined, undefined, sort, range?.start, range?.end);
    await loadIcons();
  }

  function shiftCursor(direction: -1 | 1) {
    if (period === 'all') return;
    cursorTs = shiftCursorForPeriod(period, cursorTs, direction);
    loadApps();
  }

  function setPeriod(p: AppPeriod) {
    period = p;
    cursorTs = startOfDay(Date.now() / 1000);
    loadApps();
  }

  async function loadIcons() {
    const entries = apps.filter(a => a.executable_path && !iconCache.has(a.process_name));
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

  function onSearch(e: Event) {
    search = (e.target as HTMLInputElement).value;
    loadApps();
  }

  // 展开右键菜单
  function showContextMenu(e: MouseEvent, appId: number, appName: string) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = {
      show: true,
      x: e.clientX,
      y: e.clientY,
      appId,
      appName
    };
  }

  // 关闭右键菜单
  function closeContextMenu() {
    contextMenu.show = false;
  }

  // 指派分类
  async function assignCategory(categoryId: number) {
    if (contextMenu.appId === 0) return;
    await setAppCategory(contextMenu.appId, categoryId);
    contextMenu.show = false;
    await loadApps();
  }
</script>

<!-- 引入 window 全局点击关闭监听 -->
<svelte:window onclick={closeContextMenu} oncontextmenu={closeContextMenu} />

<div class="page">
  <div class="header-strip">
    <div class="header-left">
      <h1 class="page-title">应用列表</h1>
      <p class="page-desc">
        {#if period === 'all'}
          所有已追踪的应用列表（历史累计）
        {:else}
          时间跨度：{periodLabel} 的活跃应用情况
        {/if}
      </p>
    </div>

    <!-- 顶栏时间段控制器与翻页器 -->
    <div class="header-controls">
      <!-- 迷你左右翻页导航器 -->
      {#if period !== 'all'}
        <div class="date-navigator-mini">
          <button class="nav-btn-mini" onclick={() => shiftCursor(-1)} title="前一个周期">
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
            </svg>
          </button>
          <span class="date-label-mini">{periodLabel}</span>
          <button class="nav-btn-mini" onclick={() => shiftCursor(1)} title="后一个周期">
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>
      {/if}

      <!-- 时间段 Segmented 切换器 -->
      <div class="period-switcher">
        <button class:active={period === 'day'} onclick={() => setPeriod('day')}>天</button>
        <button class:active={period === 'week'} onclick={() => setPeriod('week')}>周</button>
        <button class:active={period === 'month'} onclick={() => setPeriod('month')}>月</button>
        <button class:active={period === 'year'} onclick={() => setPeriod('year')}>年</button>
        <button class:active={period === 'all'} onclick={() => setPeriod('all')}>全部</button>
      </div>
    </div>
  </div>

  <div class="toolbar">
    <!-- 美化搜索框：包含放大镜 SVG -->
    <div class="search-wrapper">
      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5" class="search-icon">
        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input class="search-input" type="text" placeholder="搜索应用..." value={search} oninput={onSearch} />
    </div>

    <!-- 美化排序下拉框：包含向下的箭头 SVG -->
    <div class="filter-wrapper">
      <select class="filter-select" bind:value={sort} onchange={loadApps}>
        <option value="time">按时长排序</option>
        <option value="name">按名称排序</option>
      </select>
      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3.5" class="filter-arrow">
        <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
      </svg>
    </div>
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
        <!-- 绑定鼠标右键事件，打开 ContextMenu -->
        <div class="table-row" oncontextmenu={(e) => showContextMenu(e, app.id, appDisplayName(app.display_name, app.process_name))}>
          <span class="col-icon">
            {#if iconCache.get(app.process_name)}
              <img src={iconCache.get(app.process_name)} alt="" class="app-icon" />
            {:else}
              <span class="app-icon-placeholder">{app.process_name[0].toUpperCase()}</span>
            {/if}
          </span>
          <span class="col-name">{appDisplayName(app.display_name, app.process_name)}</span>
          <span class="col-path">{app.executable_path ?? '-'}</span>
          <span class="col-total">{formatDuration(app.total_seconds)}</span>
          <span class="col-category">
            {#if app.category_name}
              <!-- 还原为精致、纯净的只读分类药丸 -->
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

  <!-- 自定义 ContextMenu 右键分类设置菜单（毛玻璃质感） -->
  {#if contextMenu.show}
    <div class="context-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px;" onclick={(e) => e.stopPropagation()}>
      <div class="menu-header">{contextMenu.appName}</div>
      <div class="menu-divider"></div>
      
      <!-- 选项一：指派分类（含有二级子菜单） -->
      <div class="menu-item has-submenu">
        <div class="menu-item-content">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="menu-icon">
            <path stroke-linecap="round" stroke-linejoin="round" d="M7 7h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <span class="menu-item-text">指派分类</span>
        </div>
        <!-- 指针箭头 -->
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3" class="submenu-arrow">
          <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
        </svg>

        <!-- 二级子菜单：分类选择面板 -->
        <div class="submenu context-menu">
          <button class="menu-item clear-btn" onclick={() => assignCategory(0)}>
            <span class="menu-dot clear-dot"></span>
            <span class="menu-item-text">清除分类 (未分类)</span>
          </button>
          <div class="menu-divider"></div>
          <div class="menu-scroll-area">
            {#each cats as cat}
              <button class="menu-item" onclick={() => assignCategory(cat.id)}>
                <span class="menu-dot" style="background: {cat.color ?? '#5048E5'}"></span>
                <span class="menu-item-text">{cat.name}</span>
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- 选项二：打开文件位置（预留） -->
      <button class="menu-item disabled-menu-item" type="button">
        <div class="menu-item-content">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="menu-icon">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <span class="menu-item-text">打开文件位置</span>
        </div>
      </button>

      <!-- 选项三：排除此应用（预留） -->
      <button class="menu-item disabled-menu-item" type="button">
        <div class="menu-item-content">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="menu-icon">
            <path stroke-linecap="round" stroke-linejoin="round" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
          </svg>
          <span class="menu-item-text">排除此应用 (不追踪)</span>
        </div>
      </button>
    </div>
  {/if}
</div>

<style>
  .page { max-width: 1000px; }

  .header-strip {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.85rem;
    width: 100%;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .page-title { font-family: 'Segoe UI Variable Display','Segoe UI',sans-serif; font-weight: 600; font-size: 0.85rem; color: theme('colors.text.primary'); margin: 0; }
  .page-desc { font-family: 'Segoe UI',sans-serif; font-size: 0.45rem; color: theme('colors.text.tertiary'); margin: 0; }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  /* Segmented Control 切换器样式 */
  .period-switcher {
    display: flex;
    background: #FAF9FD;
    border: 1px solid #ECE9F5;
    border-radius: 6px;
    padding: 0.08rem;
    gap: 0.05rem;
  }

  .period-switcher button {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.52rem;
    font-weight: 600;
    color: theme('colors.text.secondary');
    border: none;
    background: transparent;
    padding: 0.2rem 0.55rem;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .period-switcher button:hover {
    color: theme('colors.primary.DEFAULT');
    background: rgba(80, 72, 229, 0.03);
  }

  .period-switcher button.active {
    background: #FFFFFF;
    color: theme('colors.primary.DEFAULT');
    box-shadow: 0 1px 3px rgba(80, 72, 229, 0.12), 0 1px 2px rgba(0, 0, 0, 0.02);
  }

  /* 迷你翻页导航器样式 */
  .date-navigator-mini {
    display: flex;
    align-items: center;
    background: #FAF9FD;
    border: 1px solid #ECE9F5;
    border-radius: 6px;
    padding: 0.12rem 0.25rem;
    gap: 0.3rem;
  }

  .nav-btn-mini {
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    cursor: pointer;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    color: theme('colors.text.secondary');
    transition: all 0.2s;
    padding: 0;
  }

  .nav-btn-mini:hover {
    background: rgba(80, 72, 229, 0.05);
    color: theme('colors.primary.DEFAULT');
  }

  .nav-btn-mini svg {
    width: 12px;
    height: 12px;
  }

  .date-label-mini {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 700;
    color: theme('colors.text.primary');
    min-width: 95px;
    text-align: center;
    white-space: nowrap;
  }

  .toolbar {
    display: flex;
    gap: 0.6rem;
    margin-bottom: 0.85rem;
    width: 100%;
  }

  /* 搜索框包装器 */
  .search-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
  }

  .search-icon {
    position: absolute;
    left: 0.55rem;
    width: 13px;
    height: 13px;
    color: theme('colors.text.tertiary');
    pointer-events: none;
    transition: color 0.2s;
  }

  .search-input {
    width: 100%;
    padding: 0.25rem 0.5rem 0.25rem 1.4rem;
    border-radius: 6px;
    border: 1px solid #ECE9F5;
    background: #FAF9FD;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    font-weight: 500;
    color: theme('colors.text.primary');
    outline: none;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .search-input::placeholder {
    color: theme('colors.text.tertiary');
    opacity: 0.85;
  }

  .search-input:focus {
    border-color: color-mix(in srgb, theme('colors.primary.DEFAULT') 50%, transparent);
    background: #FFFFFF;
    box-shadow: 0 0 0 3px color-mix(in srgb, theme('colors.primary.DEFAULT') 8%, transparent);
  }

  .search-wrapper:focus-within .search-icon {
    color: theme('colors.primary.DEFAULT');
  }

  /* 排序下拉框包装器 */
  .filter-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 110px;
    flex-shrink: 0;
  }

  .filter-select {
    width: 100%;
    padding: 0.25rem 0.8rem 0.25rem 0.45rem;
    border-radius: 6px;
    border: 1px solid #ECE9F5;
    background: #FAF9FD;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    font-weight: 600;
    color: theme('colors.text.secondary');
    outline: none;
    cursor: pointer;
    appearance: none;
    -webkit-appearance: none;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    text-align: left;
  }

  .filter-select:hover {
    border-color: color-mix(in srgb, theme('colors.primary.DEFAULT') 35%, transparent);
    color: theme('colors.primary.DEFAULT');
  }

  .filter-select:focus {
    border-color: color-mix(in srgb, theme('colors.primary.DEFAULT') 50%, transparent);
    background: #FFFFFF;
    color: theme('colors.primary.DEFAULT');
    box-shadow: 0 0 0 3px color-mix(in srgb, theme('colors.primary.DEFAULT') 8%, transparent);
  }

  .filter-arrow {
    position: absolute;
    right: 0.35rem;
    width: 8px;
    height: 8px;
    color: theme('colors.text.tertiary');
    pointer-events: none;
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1), color 0.2s;
  }

  .filter-wrapper:hover .filter-arrow {
    color: theme('colors.primary.DEFAULT');
  }

  .filter-wrapper:focus-within .filter-arrow {
    transform: rotate(180deg);
    color: theme('colors.primary.DEFAULT');
  }

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
  .col-category { width: 75px; text-align: center; }

  .cat-pill {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    font-weight: 600;
    border-radius: 4px;
    padding: 0.02rem 0.25rem;
    display: inline-block;
  }

  .table-empty { display: flex; align-items: center; justify-content: center; padding: 3rem 0; }

  /* 自定义右键 ContextMenu 样式 */
  .context-menu {
    position: fixed;
    z-index: 1000;
    min-width: 150px;
    max-width: 220px;
    background: #FFFFFF;
    border: 1px solid rgba(80, 72, 229, 0.15);
    border-radius: 8px;
    box-shadow: 0 10px 25px rgba(80, 72, 229, 0.08), 0 3px 6px rgba(0, 0, 0, 0.02);
    padding: 0.25rem 0;
    animation: menu-fade-in 0.15s cubic-bezier(0.4, 0, 0.2, 1);
  }

  @keyframes menu-fade-in {
    from { opacity: 0; transform: scale(0.95); }
    to { opacity: 1; transform: scale(1); }
  }

  .menu-header {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    font-weight: 700;
    color: #000000;
    padding: 0.25rem 0.5rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .menu-divider {
    height: 1px;
    background: rgba(80, 72, 229, 0.06);
    margin: 0.15rem 0;
  }

  .menu-scroll-area {
    max-height: 180px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgba(80, 72, 229, 0.15) transparent;
  }

  /* 自定义滚动条风格 */
  .menu-scroll-area::-webkit-scrollbar {
    width: 4px;
  }
  .menu-scroll-area::-webkit-scrollbar-thumb {
    background: rgba(80, 72, 229, 0.15);
    border-radius: 2px;
  }
  .menu-scroll-area::-webkit-scrollbar-track {
    background: transparent;
  }

  .menu-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.25rem 0.5rem;
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    box-sizing: border-box;
  }

  .menu-item:hover {
    background: rgba(80, 72, 229, 0.05);
  }

  .menu-item-content {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .menu-icon {
    width: 12px;
    height: 12px;
    color: #000000;
    flex-shrink: 0;
  }

  .menu-item:hover .menu-icon {
    color: #000000;
  }

  .menu-item-text {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    font-weight: 600;
    color: #000000;
    transition: transform 0.2s;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .menu-item:hover .menu-item-text {
    color: #000000;
    transform: translateX(1.5px);
  }

  .menu-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .clear-dot {
    background: #CCCCCC;
    border: 1px dashed #999;
  }

  .clear-btn:hover {
    background: rgba(229, 57, 53, 0.04);
  }

  /* 级联二级子菜单的核心 CSS */
  .has-submenu {
    position: relative;
  }

  .submenu-arrow {
    width: 8px;
    height: 8px;
    color: #000000;
    flex-shrink: 0;
    transition: transform 0.2s, color 0.2s;
  }

  .has-submenu:hover .submenu-arrow {
    color: #000000;
    transform: translateX(1px);
  }

  .submenu {
    display: none;
    opacity: 0;
    pointer-events: none;
    position: absolute;
    left: 100%;
    top: -5px;
    margin-left: 2px;
    transform: translateX(4px);
    transition: opacity 0.2s, transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .has-submenu:hover .submenu {
    display: block;
    opacity: 1;
    pointer-events: auto;
    transform: translateX(0);
  }

  /* 禁用（预留）的菜单项样式 */
  .disabled-menu-item {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .disabled-menu-item:hover {
    background: transparent;
  }

  .disabled-menu-item:hover .menu-item-text,
  .disabled-menu-item:hover .menu-icon {
    color: theme('colors.text.secondary');
    transform: none;
  }
</style>
