<script lang="ts">
  import { onMount } from 'svelte';
  import { getConfigValue, getDisplayName, setConfigValue, checkForUpdate as checkForUpdateApi } from '$lib/api/commands';
  import { isEnabled, enable, disable } from '@tauri-apps/plugin-autostart';
  import { getVersion } from '@tauri-apps/api/app';

  let flushInterval = $state('15');
  let autoStart = $state(false);
  let saving = $state(false);
  let windowW = $state('960');
  let windowH = $state('620');
  let savingWin = $state(false);
  let appVersion = $state('');
  let displayName = $state('踪');

  // 检查更新（手动触发，不轮询）
  type UpdateState = 'idle' | 'checking' | 'latest' | 'available' | 'error';
  let updateState = $state<UpdateState>('idle');
  let latestVersion = $state('');
  let errorMsg = $state('');
  let latestTimeout: ReturnType<typeof setTimeout> | null = null;
  const RELEASE_PAGE = 'https://github.com/tuokun/Tracer/releases/latest';

  function compareVersions(a: string, b: string): number {
    const an = a.replace(/^v/, '').split('.').map(n => parseInt(n, 10) || 0);
    const bn = b.replace(/^v/, '').split('.').map(n => parseInt(n, 10) || 0);
    const len = Math.max(an.length, bn.length);
    for (let i = 0; i < len; i++) {
      const av = an[i] ?? 0;
      const bv = bn[i] ?? 0;
      if (av !== bv) return av > bv ? 1 : -1;
    }
    return 0;
  }

  async function checkForUpdate() {
    if (updateState === 'checking' || !appVersion) return;
    if (latestTimeout) {
      clearTimeout(latestTimeout);
      latestTimeout = null;
    }
    updateState = 'checking';
    const res = await checkForUpdateApi();
    if (res.error || !res.latest_version) {
      console.error('检查更新失败:', res.error);
      errorMsg = res.error ?? '未知错误';
      updateState = 'error';
      return;
    }
    if (compareVersions(res.latest_version, appVersion) > 0) {
      latestVersion = res.latest_version.replace(/^v/, '');
      updateState = 'available';
    } else {
      updateState = 'latest';
      latestTimeout = setTimeout(() => {
        updateState = 'idle';
        latestTimeout = null;
      }, 5000);
    }
  }

  // 主题管理
  let currentTheme = $state('system');
  const themes = [
    { id: 'system', name: '跟随系统', primary: '#5048E5', bg: 'linear-gradient(135deg, #F0F0EC 50%, #0F172A 50%)' },
    { id: 'light', name: '极简浅色', primary: '#5048E5', bg: '#F0F0EC' },
    { id: 'pure', name: '雅致纯白', primary: '#5048E5', bg: '#FFFFFF', border: '#E0DCF0' },
    { id: 'dark', name: '护眼深色', primary: '#818CF8', bg: '#0F172A' },
    { id: 'ocean', name: '深海幽蓝', primary: '#0EA5E9', bg: '#0B192C' },
    { id: 'forest', name: '森林护眼', primary: '#10B981', bg: '#064E3B' }
  ];

  onMount(async () => {
    const [f, enabled, ws, t, version, name] = await Promise.all([
      getConfigValue('flush_interval_secs'),
      isEnabled(),
      getConfigValue('window_size'),
      getConfigValue('theme'),
      getVersion(),
      getDisplayName(),
    ]);
    if (f) flushInterval = String(Math.floor(parseInt(f) / 60));
    autoStart = enabled;
    if (ws) {
      const [w, h] = ws.split(',');
      if (w) windowW = w.trim();
      if (h) windowH = h.trim();
    }
    if (t) currentTheme = t;
    appVersion = version;
    displayName = name;
  });

  async function saveFlush() {
    saving = true;
    const secs = String(Math.max(60, parseInt(flushInterval) || 15) * 60);
    await setConfigValue('flush_interval_secs', secs);
    saving = false;
  }

  async function toggleAutoStart() {
    if (autoStart) {
      await disable();
    } else {
      await enable();
    }
  }

  async function saveWindowSize() {
    savingWin = true;
    const w = Math.max(400, parseInt(windowW) || 960);
    const h = Math.max(300, parseInt(windowH) || 620);
    windowW = String(w);
    windowH = String(h);
    await setConfigValue('window_size', `${w},${h}`);
    savingWin = false;
  }

  async function selectTheme(themeId: string) {
    currentTheme = themeId;
    await setConfigValue('theme', themeId);
    window.dispatchEvent(new CustomEvent('theme-changed', { detail: themeId }));
  }

  async function openExternalLink(url: string) {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener');
      await openUrl(url);
    } catch (e) {
      console.error("打开链接失败:", e);
      window.open(url, '_blank');
    }
  }

  async function openReleasePage() {
    await openExternalLink(RELEASE_PAGE);
  }
</script>

<div class="page">
  <h1 class="page-title">设置</h1>
  <p class="page-desc">应用配置和偏好</p>

  <!-- 基础设置卡片 -->
  <div class="card">
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-name">Flush 间隔</div>
        <div class="setting-desc">数据写入间隔（分钟）</div>
      </div>
      <div class="setting-value">
        <input class="setting-input" type="number" bind:value={flushInterval} min="1" />
        <span class="setting-unit">min</span>
        <button class="setting-save" onclick={saveFlush} disabled={saving}>
          {saving ? '...' : '保存'}
        </button>
      </div>
    </div>
    <div class="setting-divider"></div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-name">Idle 阈值</div>
        <div class="setting-desc">闲置判定时间（与 flush 间隔联动）</div>
      </div>
      <div class="setting-value">
        <span class="setting-static">{flushInterval} min</span>
      </div>
    </div>
    <div class="setting-divider"></div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-name">开机自启</div>
        <div class="setting-desc">系统启动时自动运行</div>
      </div>
      <div class="setting-value">
        <label class="toggle">
          <input type="checkbox" bind:checked={autoStart} onchange={toggleAutoStart} />
          <span class="toggle-slider"></span>
        </label>
      </div>
    </div>
    <div class="setting-divider"></div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-name">窗口大小</div>
        <div class="setting-desc">下次启动时生效（像素）</div>
      </div>
      <div class="setting-value">
        <input class="setting-input" type="number" bind:value={windowW} min="400" />
        <span class="setting-unit">×</span>
        <input class="setting-input" type="number" bind:value={windowH} min="300" />
        <span class="setting-unit">px</span>
        <button class="setting-save" onclick={saveWindowSize} disabled={savingWin}>
          {savingWin ? '...' : '保存'}
        </button>
      </div>
    </div>
  </div>

  <!-- 主题管理卡片 -->
  <div class="card mt-6">
    <div class="section-title">主题管理</div>
    <div class="theme-grid">
      {#each themes as t}
        <button 
          class="theme-card" 
          class:active={currentTheme === t.id}
          onclick={() => selectTheme(t.id)}
        >
          <!-- 预设小圆盘色预览 -->
          <div 
            class="theme-preview" 
            style="background: {t.bg}; border: {t.border ? `1px solid ${t.border}` : 'none'}"
          >
            <div class="theme-preview-dot" style="background: {t.primary}"></div>
          </div>
          <span class="theme-name">{t.name}</span>
        </button>
      {/each}
    </div>
  </div>

  <!-- 关于应用卡片 -->
  <div class="card mt-6">
    <div class="about-section">
      <div class="section-title">关于 {displayName}</div>
      <p class="about-desc">
        {displayName} 是一款轻量级、无感知的个人电脑活动效率分析工具。它可以自动记录您在各应用下的专注时间，并提供优雅的统计与热力图分析，帮助您理清每天的时间走向。
      </p>
      <div class="about-metadata">
        <div class="metadata-row">
          <span class="metadata-label">当前版本</span>
          <span class="metadata-value">{appVersion ? `v${appVersion}` : '—'}</span>
        </div>
        <div class="metadata-row">
          <span class="metadata-label">检查更新</span>
          <div class="update-action">
            {#if updateState === 'checking'}
              <span class="update-status"><span class="spinner"></span>正在检查…</span>
            {:else if updateState === 'latest'}
              <span class="update-status success">✓ 已是最新版本</span>
            {:else if updateState === 'available'}
              <span class="update-status">发现 v{latestVersion}</span>
              <button class="setting-save" onclick={openReleasePage}>前往下载</button>
            {:else if updateState === 'error'}
              <span class="update-status danger" title={errorMsg}>⚠ {errorMsg}</span>
              <button class="setting-save" onclick={checkForUpdate}>重试</button>
            {:else}
              <button class="setting-save" onclick={checkForUpdate}>检查更新</button>
            {/if}
          </div>
        </div>
        <div class="metadata-row">
          <span class="metadata-label">项目源码</span>
          <button class="link-btn" onclick={() => openExternalLink('https://github.com/cgfhsc/tracer')}>
            GitHub 仓库
          </button>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .page { max-width: 600px; margin: 0 auto; }

  .page-title {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 700;
    font-size: 0.85rem;
    color: var(--color-text-primary);
    margin: 0;
  }

  .page-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: var(--color-text-secondary);
    margin-top: 0.15rem;
    margin-bottom: 0.85rem;
  }

  .mt-6 {
    margin-top: 0.85rem;
  }

  .section-title {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.55rem;
    font-weight: 700;
    color: var(--color-text-primary);
    margin-bottom: 0.6rem;
  }

  .card {
    padding: 0.85rem 1.15rem;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 0;
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .setting-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.55rem;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .setting-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    color: var(--color-text-secondary);
  }

  .setting-value {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .setting-input {
    width: 65px;
    padding: 0.25rem 0.45rem;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.52rem;
    font-weight: 600;
    color: var(--color-text-primary);
    text-align: center;
    outline: none;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .setting-input:focus {
    border-color: var(--color-primary);
    background: var(--color-surface);
    box-shadow: 0 0 0 3px var(--color-primary-hover);
  }

  .setting-unit {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .setting-save {
    padding: 0.25rem 0.65rem;
    border-radius: 6px;
    border: none;
    background: linear-gradient(135deg, var(--color-primary) 0%, var(--color-primary) 100%);
    color: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 2px 6px var(--color-primary-hover);
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .setting-save:hover:not(:disabled) {
    transform: translateY(-1px);
    opacity: 0.95;
  }

  .setting-save:active:not(:disabled) {
    transform: translateY(0.5px);
  }

  .setting-save:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    box-shadow: none;
  }

  .setting-static {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.52rem;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .setting-divider {
    height: 1px;
    background: linear-gradient(to right, var(--color-border) 0%, transparent 100%);
  }

  /* 主题网格样式 */
  .theme-grid {
    @apply grid grid-cols-3 gap-3 mt-3;
  }

  .theme-card {
    @apply flex flex-col items-center p-3 rounded-lg border border-border bg-surface cursor-pointer transition-all duration-200;
  }

  .theme-card:hover {
    @apply border-primary scale-[1.02];
  }

  .theme-card.active {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px var(--color-primary-hover);
  }

  .theme-preview {
    @apply w-12 h-8 rounded relative overflow-hidden mb-2 shadow-sm flex items-center justify-center;
  }

  .theme-preview-dot {
    @apply w-3 h-3 rounded-full shadow-sm;
  }

  .theme-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  /* 关于页面样式 */
  .about-section {
    @apply flex flex-col;
  }

  .about-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    line-height: 1.5;
    color: var(--color-text-secondary);
    margin-top: 0.1rem;
    margin-bottom: 0.85rem;
  }

  .about-metadata {
    display: grid;
    grid-template-columns: max-content max-content;
    justify-content: space-between;
    align-items: center;
    row-gap: 10px;
    border-top: 1px solid var(--color-border);
    padding-top: 14px;
  }

  .metadata-row {
    display: contents;
  }

  .metadata-label {
    justify-self: start;
  }

  .metadata-value,
  .update-action,
  .link-btn {
    justify-self: center;
  }

  .metadata-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .metadata-value {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.48rem;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .link-btn {
    @apply bg-transparent border-none p-0 cursor-pointer font-bold transition-all;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.48rem;
    color: var(--color-primary);
  }

  .link-btn:hover {
    @apply underline;
  }

  /* 开关滑动条样式 */
  .toggle {
    display: inline-flex;
    align-items: center;
    cursor: pointer;
  }
  
  .toggle input {
    display: none;
  }
  
  .toggle-slider {
    width: 34px;
    height: 18px;
    background: var(--color-border);
    border-radius: 9px;
    position: relative;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }
  
  .toggle-slider::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #FFFFFF;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }
  
  .toggle input:checked + .toggle-slider {
    background: var(--color-primary);
  }
  
  .toggle input:checked + .toggle-slider::after {
    left: 18px;
  }

  .toggle:hover .toggle-slider {
    box-shadow: 0 0 0 2px var(--color-primary-hover);
  }

  /* 检查更新 */
  .update-action {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .update-status {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    font-weight: 600;
    color: var(--color-text-secondary);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .update-status.success { color: var(--color-success); }
  .update-status.danger { color: #EF4444; }

  .spinner {
    width: 8px;
    height: 8px;
    border: 1.5px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin { to { transform: rotate(360deg); } }
</style>
