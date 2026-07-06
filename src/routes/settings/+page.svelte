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
  .page { max-width: 600px; margin: 0 auto; }

  .page-title {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 700;
    font-size: 0.85rem;
    color: theme('colors.text.primary');
    margin: 0;
  }

  .page-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.secondary');
    margin-top: 0.15rem;
    margin-bottom: 0.85rem;
  }

  .card {
    padding: 0.85rem 1.15rem;
    background: #FFFFFF;
    border: 1px solid rgba(80, 72, 229, 0.08);
    border-radius: 12px;
    box-shadow: 0 10px 30px rgba(80, 72, 229, 0.03), 0 1px 3px rgba(0, 0, 0, 0.01);
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
    color: #000000;
  }

  .setting-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    color: theme('colors.text.secondary');
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
    border: 1px solid #ECE9F5;
    background: #FAF9FD;
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.52rem;
    font-weight: 600;
    color: #000000;
    text-align: center;
    outline: none;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .setting-input:focus {
    border-color: color-mix(in srgb, theme('colors.primary.DEFAULT') 50%, transparent);
    background: #FFFFFF;
    box-shadow: 0 0 0 3px color-mix(in srgb, theme('colors.primary.DEFAULT') 8%, transparent);
  }

  .setting-unit {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.secondary');
  }

  .setting-save {
    padding: 0.25rem 0.65rem;
    border-radius: 6px;
    border: none;
    background: linear-gradient(135deg, theme('colors.primary.DEFAULT') 0%, color-mix(in srgb, theme('colors.primary.DEFAULT') 82%, #000) 100%);
    color: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.45rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 2px 6px color-mix(in srgb, theme('colors.primary.DEFAULT') 12%, transparent);
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .setting-save:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px color-mix(in srgb, theme('colors.primary.DEFAULT') 20%, transparent);
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
    color: theme('colors.text.secondary');
  }

  .setting-divider {
    height: 1px;
    background: linear-gradient(to right, rgba(80, 72, 229, 0.08) 0%, rgba(80, 72, 229, 0.01) 100%);
  }

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
    background: #ECE9F5;
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
    background: theme('colors.primary.DEFAULT');
  }
  
  .toggle input:checked + .toggle-slider::after {
    left: 18px;
  }

  .toggle:hover .toggle-slider {
    box-shadow: 0 0 0 2px color-mix(in srgb, theme('colors.primary.DEFAULT') 8%, transparent);
  }
</style>
