<script lang="ts">
  import { onMount } from 'svelte';
  import { getConfigValue, setConfigValue } from '$lib/api/commands';
  import { isEnabled, enable, disable } from '@tauri-apps/plugin-autostart';

  let flushInterval = $state('15');
  let autoStart = $state(false);
  let saving = $state(false);
  let windowW = $state('960');
  let windowH = $state('540');
  let savingWin = $state(false);

  onMount(async () => {
    const f = await getConfigValue('flush_interval_secs');
    if (f) flushInterval = String(Math.floor(parseInt(f) / 60));
    autoStart = await isEnabled();
    const ws = await getConfigValue('window_size');
    if (ws) {
      const [w, h] = ws.split(',');
      if (w) windowW = w.trim();
      if (h) windowH = h.trim();
    }
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
    const h = Math.max(300, parseInt(windowH) || 540);
    windowW = String(w);
    windowH = String(h);
    await setConfigValue('window_size', `${w},${h}`);
    savingWin = false;
  }
</script>

<div class="page">
  <h1 class="page-title">设置</h1>
  <p class="page-desc">应用配置和偏好</p>

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
</div>

<style>
  .page { max-width: 700px; }

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

  .card {
    padding: 0.75rem 1rem;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0;
  }

  .setting-info { display: flex; flex-direction: column; gap: 0.125rem; }

  .setting-name {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.primary');
  }

  .setting-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: theme('colors.text.tertiary');
  }

  .setting-value {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .setting-input {
    width: 50px;
    padding: 0.25rem 0.375rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
    text-align: center;
    outline: none;
  }

  .setting-input:focus { border-color: #5048E5; }

  .setting-unit {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
  }

  .setting-save {
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    border: none;
    background: theme('colors.primary.DEFAULT');
    color: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    cursor: pointer;
  }

  .setting-save:hover { opacity: 0.9; }
  .setting-save:disabled { opacity: 0.5; }

  .setting-static {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
  }

  .setting-divider { height: 1px; background: theme('colors.border'); }

  .toggle { display: inline-flex; align-items: center; cursor: pointer; }
  .toggle input { display: none; }
  .toggle-slider { width: 28px; height: 16px; background: #D0C8E0; border-radius: 8px; position: relative; transition: 0.2s; }
  .toggle-slider::after { content:''; position:absolute; top:2px; left:2px; width:12px; height:12px; border-radius:50%; background:#fff; transition:0.2s; }
  .toggle input:checked + .toggle-slider { background: theme('colors.primary.DEFAULT'); }
  .toggle input:checked + .toggle-slider::after { left:14px; }
</style>
