<script lang="ts">
  import { onMount } from 'svelte';
  import { getConfigValue, getDisplayName, setConfigValue, checkForUpdate as checkForUpdateApi, configureSync, getSyncOverview, syncNow, cancelSync, disconnectSync, renameSyncDevice, changeSyncPassword, resetSyncSpace, forceResetSyncSpace, rebuildRemoteYear, restoreLocalYear } from '$lib/api/commands';
  import type { SyncOverview } from '$lib/api/types';
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
  let syncOverview = $state<SyncOverview | null>(null);
  let syncEndpoint = $state(''); let syncUsername = $state(''); let webdavPassword = $state(''); let syncDirectory = $state(''); let syncPassword = $state(''); let syncDeviceName = $state(''); let syncYear = $state(new Date().getFullYear()); let syncMessage = $state(''); let syncBusy = $state(false); let allowHttp = $state(false);
  let syncModalOpen = $state(false);
  let forceResetConfirmOpen = $state(false);
  type ManagementAction = 'password' | 'rebuild' | 'restore' | 'reset';
  let managementAction = $state<ManagementAction | null>(null);
  let managementBusy = $state(false);
  let managementError = $state('');
  let syncHistoryOpen = $state(false);
  let newSyncPassword = $state('');
  let newSyncPasswordConfirm = $state('');
  let yearMenuOpen = $state(false);
  let syncSaveState = $state<'idle' | 'saving' | 'success' | 'failed'>('idle');
  let syncActivityState = $state<'ready' | 'running' | 'success' | 'failed'>('ready');
  let syncActivityTimer: ReturnType<typeof setTimeout> | null = null;
  type SyncField = 'endpoint' | 'webdavPassword' | 'directory' | 'syncPassword';
  let syncFieldErrors = $state<Partial<Record<SyncField, string>>>({});

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
    await refreshSync();
  });

  async function refreshSync() { try { syncOverview = await getSyncOverview(); syncDeviceName = syncOverview.current_device_name; if (syncOverview.years.length && !syncOverview.years.includes(syncYear)) syncYear = syncOverview.years[0]; } catch (e) { syncMessage = String(e); } }
  function clearSyncFieldError(field: SyncField) {
    if (!syncFieldErrors[field]) return;
    const next = { ...syncFieldErrors };
    delete next[field];
    syncFieldErrors = next;
  }

  function validateSyncFields() {
    const errors: Partial<Record<SyncField, string>> = {};
    const endpoint = syncEndpoint.trim();
    if (!endpoint) {
      errors.endpoint = '请输入 WebDAV 地址';
    } else {
      try {
        const url = new URL(endpoint);
        if (url.protocol !== 'http:' && url.protocol !== 'https:') errors.endpoint = '地址必须以 http:// 或 https:// 开头';
      } catch {
        errors.endpoint = 'WebDAV 地址格式不正确';
      }
    }
    if (syncDirectory.split('/').some(part => part === '..')) errors.directory = '同步目录不能包含 ..';
    if (!syncPassword) errors.syncPassword = '同步密码不能为空';
    if (endpoint.startsWith('http://') && !allowHttp) errors.endpoint = '使用 HTTP 前需要确认风险';
    syncFieldErrors = errors;
    return Object.keys(errors).length === 0;
  }

  function applySyncFieldError(error: unknown) {
    const message = String(error);
    let field: SyncField | null = null;
    if (message.includes('WebDAV 地址') || message.includes('HTTP WebDAV')) field = 'endpoint';
    else if (message.includes('同步目录')) field = 'directory';
    else if (message.includes('两次输入') || message.includes('密码不一致')) field = 'syncPassword';
    else if (message.includes('同步密码') || message.includes('密码错误') || message.includes('无法解密')) field = 'syncPassword';
    else if (message.includes('HTTP 401') || message.includes('HTTP 403')) field = 'webdavPassword';
    if (!field) return false;
    syncFieldErrors = { ...syncFieldErrors, [field]: message };
    syncMessage = '';
    return true;
  }

  async function saveSync() { if(!validateSyncFields()){syncSaveState='failed';return;} syncBusy = true; syncSaveState = 'saving'; syncMessage = ''; try { const result = await configureSync({ endpoint: syncEndpoint, username: syncUsername, webdav_password: webdavPassword, directory: syncDirectory.trim() || null, sync_password: syncPassword, sync_password_confirm: syncPassword, device_name: syncDeviceName.trim() || syncOverview?.current_device_name || 'Windows PC', allow_insecure_http: allowHttp }); syncSaveState='success';syncMessage = result.joined_existing ? '已加入现有同步空间' : '已创建同步空间'; webdavPassword='';syncPassword='';await refreshSync(); } catch(e){syncSaveState='failed';if(!applySyncFieldError(e))syncMessage=String(e);} finally{syncBusy=false;} }
  function setSyncActivity(state: 'ready' | 'running' | 'success' | 'failed') {
    if (syncActivityTimer) { clearTimeout(syncActivityTimer); syncActivityTimer = null; }
    syncActivityState = state;
    if (state === 'success') syncActivityTimer = setTimeout(() => { syncActivityState = 'ready'; syncActivityTimer = null; }, 2500);
  }
  async function runSync(){syncBusy=true;syncMessage='';setSyncActivity('running');const poll=setInterval(refreshSync,500);try{await syncNow(syncYear);await refreshSync();setSyncActivity('success');}catch(e){syncMessage=String(e);setSyncActivity('failed');}finally{clearInterval(poll);syncBusy=false;}}
  async function stopSync(){await cancelSync();syncMessage='正在安全取消…';}
  async function saveDeviceName(){try{await renameSyncDevice(syncDeviceName);syncMessage='设备名称已更新，将在下次同步时发布';await refreshSync();}catch(e){syncMessage=String(e);}}
  async function disconnect(){if(!confirm('仅断开本机同步配置？本地数据和远端文件都不会删除。'))return;await disconnectSync();syncMessage='已断开同步';await refreshSync();}
  function openManagementModal(action: ManagementAction) {
    managementAction = action;
    managementError = '';
    newSyncPassword = '';
    newSyncPasswordConfirm = '';
  }
  function closeManagementModal() { if (!managementBusy) managementAction = null; }
  async function submitManagementAction() {
    if (!managementAction) return;
    managementError = '';
    if ((managementAction === 'password' || managementAction === 'reset') && !newSyncPassword) { managementError = '请输入新的同步密码'; return; }
    if ((managementAction === 'password' || managementAction === 'reset') && newSyncPassword !== newSyncPasswordConfirm) { managementError = '两次输入的同步密码不一致'; return; }
    managementBusy = true;
    try {
      if (managementAction === 'password') await changeSyncPassword(null, newSyncPassword, newSyncPasswordConfirm);
      else if (managementAction === 'rebuild') await rebuildRemoteYear(syncYear, true);
      else if (managementAction === 'restore') await restoreLocalYear(syncYear, true);
      else await resetSyncSpace(newSyncPassword, newSyncPasswordConfirm, [syncYear], true);
      managementAction = null;
      await refreshSync();
    } catch (e) {
      managementError = String(e);
    } finally {
      managementBusy = false;
    }
  }
  function requestForceReset() { if (validateSyncFields()) forceResetConfirmOpen = true; }
  async function forceResetSpace(){forceResetConfirmOpen=false;syncBusy=true;syncMessage='';try{await forceResetSyncSpace({endpoint:syncEndpoint,username:syncUsername,webdav_password:webdavPassword,directory:syncDirectory.trim()||null,sync_password:syncPassword,sync_password_confirm:syncPassword,device_name:syncDeviceName.trim() || syncOverview?.current_device_name || 'Windows PC',allow_insecure_http:allowHttp},[syncYear],true);syncMessage='远端同步空间已强制重置';webdavPassword='';syncPassword='';await refreshSync();}catch(e){if(!applySyncFieldError(e))syncMessage=String(e);}finally{syncBusy=false;}}

  function openSyncModal() {
    syncMessage = '';
    syncFieldErrors = {};
    syncSaveState = 'idle';
    syncModalOpen = true;
  }

  function closeSyncModal() {
    forceResetConfirmOpen = false;
    managementAction = null;
    syncHistoryOpen = false;
    syncModalOpen = false;
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    if (managementAction) { closeManagementModal(); return; }
    if (syncHistoryOpen) { syncHistoryOpen = false; return; }
    if (yearMenuOpen) { yearMenuOpen = false; return; }
    if (syncModalOpen) closeSyncModal();
  }

  function availableSyncYears() {
    return Array.from(new Set([new Date().getFullYear(), ...(syncOverview?.years ?? [])])).sort((a, b) => b - a);
  }

  function formatSyncTime(timestamp: number) {
    const date = new Date(timestamp * 1000);
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  }

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

<svelte:window onkeydown={handleWindowKeydown} />

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

  <div class="card mt-6 sync-entry">
    <div class="setting-info">
      <div class="setting-name">多端同步</div>
      <div class="setting-desc">
        {#if syncOverview?.configured}
          {syncOverview.current_device_name} · {syncOverview.status.running ? syncOverview.status.phase : '已配置'}
        {:else}
          未配置，当前仅保存在本机
        {/if}
      </div>
    </div>
    <button class="setting-save" onclick={openSyncModal}>管理同步</button>
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

{#if syncModalOpen}
  <dialog
    class="sync-modal-backdrop"
    open
    aria-labelledby="sync-modal-title"
    onclick={(event) => { if (event.target === event.currentTarget) closeSyncModal(); }}
  >
    <div class="sync-modal-card">
      <div class="sync-modal-header">
        <div>
          <h2 id="sync-modal-title">多端同步</h2>
          <p>{syncOverview?.configured ? '管理本机与 WebDAV 中的加密数据' : '配置 WebDAV 后手动同步年度数据'}</p>
        </div>
        <button class="sync-modal-close" aria-label="关闭" onclick={closeSyncModal}>&times;</button>
      </div>

      <div class="sync-modal-body">
        {#if syncOverview?.configured}
          <div class="sync-configured-form">
            <div class="sync-device-row">
              <label class="sync-control-field"><span class="sync-label">设备名称</span><input class="sync-input" bind:value={syncDeviceName} /></label>
              <button class="setting-save compact-action" onclick={saveDeviceName}>保存名称</button>
              <button class="plain-btn compact-action" onclick={disconnect}>断开同步</button>
            </div>
            <div class="sync-year-row">
              <label class="sync-control-field"><span class="sync-label">同步年份</span><div class="sync-year-picker">
                <button class="sync-year-trigger" type="button" aria-haspopup="listbox" aria-expanded={yearMenuOpen} onclick={() => yearMenuOpen = !yearMenuOpen}>{syncYear}<span class="sync-year-chevron" aria-hidden="true"></span></button>
                {#if yearMenuOpen}
                  <div class="sync-year-menu" role="listbox" aria-label="同步年份">
                    {#each availableSyncYears() as year}
                      <button type="button" role="option" aria-selected={syncYear === year} class:selected={syncYear === year} onclick={() => { syncYear = year; yearMenuOpen = false; }}>{year}</button>
                    {/each}
                  </div>
                {/if}
              </div></label>
              <div class="sync-year-actions"><button class="setting-save compact-action" onclick={runSync} disabled={syncBusy || syncOverview.status.running}>立即同步</button>{#if syncOverview.status.running}<button class="danger-btn compact-action" onclick={stopSync}>取消</button>{/if}</div>
              <div class:running={syncOverview.status.running || syncActivityState === 'running'} class:success={syncActivityState === 'success'} class:failed={syncActivityState === 'failed'} class="sync-status-chip" title={syncActivityState === 'failed' ? syncMessage : undefined}>
                {#if syncOverview.status.running || syncActivityState === 'running'}<span class="status-spinner" aria-hidden="true"></span><span>同步中</span>
                {:else if syncActivityState === 'success'}<span class="sync-status-symbol">✓</span><span>已完成</span>
                {:else if syncActivityState === 'failed'}<span class="sync-status-symbol">!</span><span>同步失败</span>
                {:else}<span class="sync-status-dot"></span><span>就绪{syncOverview.insecure_http ? ' · HTTP' : ''}</span>{/if}
              </div>
            </div>
          </div>
          <div class="sync-actions management"><button class="plain-btn" onclick={() => openManagementModal('restore')}>恢复本地年份</button><button class="plain-btn" onclick={() => openManagementModal('rebuild')}>重建年度包</button><button class="plain-btn" onclick={() => openManagementModal('password')}>修改密码</button><button class="danger-btn" onclick={() => openManagementModal('reset')}>重置同步空间</button></div>
          {#if syncOverview.history[0]}
            {@const latestSync = syncOverview.history[0]}
            <div class="sync-history-latest">
              <span>{formatSyncTime(latestSync.created_at)}</span>
              <strong class:history-failed={!latestSync.success}>{latestSync.success ? (latestSync.imported_segments > 0 ? `导入 ${latestSync.imported_segments} 条` : '同步成功') : '同步失败'}</strong>
              <button class="history-details-btn" onclick={() => syncHistoryOpen = true}>查看详情</button>
            </div>
          {/if}
        {:else}
          <div class="sync-form">
            <label class="sync-field sync-field-full">
              <span class="sync-label">WebDAV 地址</span>
              <input class="sync-input" class:input-error={syncFieldErrors.endpoint} placeholder="https://..." bind:value={syncEndpoint} oninput={() => clearSyncFieldError('endpoint')}/>
              {#if syncFieldErrors.endpoint}<span class="field-error">{syncFieldErrors.endpoint}</span>{/if}
            </label>
            <label class="sync-field">
              <span class="sync-label">用户名</span>
              <input class="sync-input" placeholder="WebDAV 账号" bind:value={syncUsername}/>
            </label>
            <label class="sync-field">
              <span class="sync-label">应用密码</span>
              <input class="sync-input" class:input-error={syncFieldErrors.webdavPassword} type="password" placeholder="WebDAV 密码或应用密码" bind:value={webdavPassword} oninput={() => clearSyncFieldError('webdavPassword')}/>
              {#if syncFieldErrors.webdavPassword}<span class="field-error">{syncFieldErrors.webdavPassword}</span>{/if}
            </label>
            <label class="sync-field sync-field-full">
              <span class="sync-label">同步目录 <small>（可选，留空自动使用 tracer-sync）</small></span>
              <input class="sync-input" class:input-error={syncFieldErrors.directory} placeholder="例如：tracer-sync" bind:value={syncDirectory} oninput={() => clearSyncFieldError('directory')}/>
              {#if syncFieldErrors.directory}<span class="field-error">{syncFieldErrors.directory}</span>{/if}
            </label>
            <label class="sync-field sync-field-full">
              <span class="sync-label">本地数据加/解密密码</span>
              <input class="sync-input" class:input-error={syncFieldErrors.syncPassword} type="password" placeholder="用于加密和解密同步数据" bind:value={syncPassword} oninput={() => clearSyncFieldError('syncPassword')}/>
              {#if syncFieldErrors.syncPassword}<span class="field-error">{syncFieldErrors.syncPassword}</span>{/if}
            </label>
            <label class="sync-field sync-field-full">
              <span class="sync-label">设备备注名 <small>（可选，留空使用本机名称）</small></span>
              <input class="sync-input" placeholder="例如：公司电脑" bind:value={syncDeviceName}/>
            </label>
            <div class="sync-final-row">
              <div class="sync-final-actions">
                <button class="setting-save compact-action" onclick={saveSync} disabled={syncBusy}>保存</button>
                <button class="danger-btn compact-action" onclick={requestForceReset} disabled={syncBusy}>重置</button>
                <span class="save-status" aria-live="polite">
                  {#if syncSaveState === 'saving'}
                    <span class="status-spinner" aria-label="正在验证"></span>
                  {:else if syncSaveState === 'success'}
                    <span class="status-success" aria-label="验证成功">✓</span>
                  {:else if syncSaveState === 'failed'}
                    <span class="status-failed" aria-label="验证失败">!</span>
                  {/if}
                </span>
              </div>
            </div>
            {#if syncEndpoint.startsWith('http://')}<label class="http-warning"><input type="checkbox" bind:checked={allowHttp}/>我了解 HTTP 会暴露 WebDAV 用户名和密码，仍要使用</label>{/if}
            <p class="sync-form-message" class:visible={syncMessage}>{syncMessage}</p>
          </div>
        {/if}
      </div>
    </div>
  </dialog>
{/if}

{#if forceResetConfirmOpen}
  <dialog class="confirm-backdrop" open aria-labelledby="force-reset-title">
    <div class="confirm-card" role="alertdialog" aria-modal="true" aria-describedby="force-reset-desc">
      <h2 id="force-reset-title">重置远端同步空间？</h2>
      <p id="force-reset-desc">旧 manifest 和全部年度包将被直接替换，其他设备必须使用新密码重新加入。此操作不可撤销。</p>
      <div class="confirm-actions">
        <button class="plain-btn" onclick={() => forceResetConfirmOpen = false}>取消</button>
        <button class="danger-confirm" onclick={forceResetSpace}>确认重置</button>
      </div>
    </div>
  </dialog>
{/if}

{#if managementAction}
  <dialog class="confirm-backdrop" open aria-labelledby="management-dialog-title" onclick={(event) => { if (event.target === event.currentTarget) closeManagementModal(); }}>
    <div class:compact-management-dialog={managementAction === 'rebuild' || managementAction === 'restore'} class="management-dialog-card" role={managementAction === 'reset' ? 'alertdialog' : 'dialog'} aria-modal="true" aria-describedby="management-dialog-desc">
      <div class="management-dialog-header">
        <div>
          <h2 id="management-dialog-title">{managementAction === 'password' ? '修改同步密码' : managementAction === 'rebuild' ? `重建 ${syncYear} 年度包？` : managementAction === 'restore' ? `恢复本机 ${syncYear} 年数据？` : '重置同步空间？'}</h2>
          <p id="management-dialog-desc">{managementAction === 'password' ? '修改后，其他设备需要使用新密码解密同步数据。' : managementAction === 'rebuild' ? `将使用本机 ${syncYear} 年的数据覆盖远端年度包。` : managementAction === 'restore' ? `将清理本机 ${syncYear} 年的数据，再从远端年度包重新导入。` : '远端同步空间将被全量替换，其他设备必须使用新密码重新加入。'}</p>
        </div>
        <button class="sync-modal-close" aria-label="关闭" onclick={closeManagementModal}>&times;</button>
      </div>
      <div class="management-dialog-body">
        {#if managementAction === 'password' || managementAction === 'reset'}
          <label class="password-field"><span>新同步密码</span><input class="sync-input" type="password" bind:value={newSyncPassword} /></label>
          <label class="password-field"><span>确认新同步密码</span><input class="sync-input" type="password" bind:value={newSyncPasswordConfirm} /></label>
        {:else if managementAction === 'rebuild'}
          <p class="management-dialog-note">适用于远端年度包损坏，或需要明确以本机记录为准的情况。其他设备尚未进入本机的数据可能暂时丢失。</p>
        {:else}
          <p class="management-dialog-note">仅影响所选年份。请确认远端年度包可用，本机该年份统计将随数据一同重建。</p>
        {/if}
        {#if managementAction === 'reset'}<p class="management-dialog-warning">不会备份现有远端数据，此操作不可撤销。</p>{/if}
        {#if managementError}<p class="password-error">{managementError}</p>{/if}
      </div>
      <div class="management-dialog-actions">
        <button class="plain-btn compact-action" onclick={closeManagementModal} disabled={managementBusy}>取消</button>
        <button class:danger-confirm={managementAction === 'reset'} class:setting-save={managementAction !== 'reset'} class="compact-action" onclick={submitManagementAction} disabled={managementBusy}>{managementBusy ? '处理中…' : managementAction === 'password' ? '保存' : managementAction === 'rebuild' ? '确认重建' : managementAction === 'restore' ? '确认恢复' : '确认重置'}</button>
      </div>
    </div>
  </dialog>
{/if}

{#if syncHistoryOpen && syncOverview}
  <dialog class="confirm-backdrop" open aria-labelledby="sync-history-title" onclick={(event) => { if (event.target === event.currentTarget) syncHistoryOpen = false; }}>
    <div class="history-dialog-card" role="dialog" aria-modal="true">
      <div class="management-dialog-header">
        <div><h2 id="sync-history-title">同步详情</h2><p>最近 {syncOverview.history.length} 条同步记录</p></div>
        <button class="sync-modal-close" aria-label="关闭" onclick={() => syncHistoryOpen = false}>&times;</button>
      </div>
      <div class="history-dialog-list">
        {#each syncOverview.history as item}
          <div><span>{formatSyncTime(item.created_at)}</span><span class:history-failed={!item.success}>{item.success ? (item.imported_segments > 0 ? `导入 ${item.imported_segments} 条` : '同步成功') : `失败：${item.error_summary ?? '未知错误'}`}</span></div>
        {/each}
      </div>
    </div>
  </dialog>
{/if}

<style>
  .page { max-width: 600px; margin: 0 auto; }

  .page-title {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 700;
    font-size: var(--font-size-page-title);
    color: var(--color-text-primary);
    margin: 0;
  }

  .page-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: var(--font-size-body);
    color: var(--color-text-secondary);
    margin-top: 0.15rem;
    margin-bottom: 0.85rem;
  }

  .mt-6 {
    margin-top: 0.85rem;
  }

  .section-title {
    font-family: 'Segoe UI', sans-serif;
    font-size: var(--font-size-section-title);
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
    font-size: var(--font-size-section-title);
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .setting-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: var(--font-size-body);
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
    background: color-mix(in srgb, var(--color-surface) 86%, white 14%);
    font-family: 'JetBrains Mono', monospace;
    font-size: var(--font-size-body);
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
    font-size: var(--font-size-body);
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
    font-size: var(--font-size-body);
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
    font-size: var(--font-size-body);
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .sync-entry { display:flex; align-items:center; justify-content:space-between; gap:1rem; }
  .sync-form { display:grid; grid-template-columns:1fr 1fr; gap:.5rem; }
  .sync-field-full { grid-column:1/-1; }
  .sync-final-row { grid-column:1/-1; display:flex; justify-content:flex-end; align-items:center; }
  .sync-final-actions { width:50%; display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr) 1rem; align-items:center; align-self:center; gap:.35rem; }
  .sync-field { position:relative; min-width:0; }
  .sync-label { display:block; height:.52rem; margin-bottom:.22rem; color:var(--color-text-secondary); font:600 var(--font-size-body)/.52rem 'Segoe UI',sans-serif; }
  .sync-label small { color:var(--color-text-tertiary); font:500 var(--font-size-caption)/1 'Segoe UI',sans-serif; }
  .sync-input { display:block; width:100%; height:1.36rem; box-sizing:border-box; padding:.35rem .5rem; border:1px solid var(--color-border); border-radius:6px; background:color-mix(in srgb, var(--color-surface) 86%, white 14%); color:var(--color-text-primary); font:500 var(--font-size-body) 'Segoe UI',sans-serif; outline:none; }
  .sync-input:focus { border-color:var(--color-primary); box-shadow:0 0 0 3px var(--color-primary-hover); }
  .sync-input.input-error { padding-right:48%; border-color:#C17A82; box-shadow:0 0 0 2px color-mix(in srgb, #C17A82 18%, transparent); }
  .field-error { position:absolute; top:calc(.52rem + .22rem + .68rem); right:.45rem; max-width:44%; transform:translateY(-50%); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:#A95760; pointer-events:none; font:600 var(--font-size-caption)/1.2 'Segoe UI',sans-serif; }
  .sync-form-message { grid-column:1/-1; margin:0; opacity:0; color:#A95B63; font:500 var(--font-size-caption)/1.4 'Segoe UI',sans-serif; transition:opacity .15s ease; }
  .sync-form-message.visible { opacity:1; }
  .sync-configured-form { display:flex; flex-direction:column; gap:.72rem; }
  .sync-device-row { display:grid; grid-template-columns:minmax(0,1fr) 4.8rem 4.8rem; align-items:end; gap:.6rem; }
  .sync-year-row { display:grid; grid-template-columns:minmax(0,1fr) 4.8rem 4.8rem; align-items:end; gap:.6rem; }
  .sync-control-field { display:block; min-width:0; }
  .sync-year-actions { display:flex; align-items:center; gap:.35rem; }
  .sync-year-actions .compact-action { min-width:4.8rem; }
  .sync-year-picker { position:relative; width:100%; }
  .sync-year-trigger { position:relative; display:flex; align-items:center; justify-content:center; width:100%; height:1.36rem; padding:0 1rem 0 .5rem; border:1px solid var(--color-border); border-radius:6px; background:color-mix(in srgb, var(--color-surface) 86%, white 14%); color:var(--color-text-primary); cursor:pointer; font:600 var(--font-size-body) 'Segoe UI',sans-serif; }
  .sync-year-trigger:focus { border-color:var(--color-primary); box-shadow:0 0 0 3px var(--color-primary-hover); outline:none; }
  .sync-year-chevron { position:absolute; right:.48rem; top:50%; width:.32rem; height:.32rem; border-right:2px solid var(--color-text-secondary); border-bottom:2px solid var(--color-text-secondary); transform:translateY(-70%) rotate(45deg); pointer-events:none; }
  .sync-year-menu { position:absolute; left:0; right:0; top:calc(100% + .2rem); z-index:5; padding:.2rem; border:1px solid var(--color-border); border-radius:6px; background:var(--color-surface); box-shadow:0 8px 18px rgba(8,12,24,.16); }
  .sync-year-menu button { display:block; width:100%; height:1.2rem; border:0; border-radius:4px; background:transparent; color:var(--color-text-primary); cursor:pointer; font:600 var(--font-size-body) 'Segoe UI',sans-serif; }
  .sync-year-menu button:hover,.sync-year-menu button.selected { background:var(--color-primary-hover); color:var(--color-primary); }
  .sync-status-chip { align-self:end; display:flex; align-items:center; justify-content:center; gap:.28rem; min-width:0; height:1.36rem; box-sizing:border-box; border:1px solid color-mix(in srgb,var(--color-primary) 22%,var(--color-border)); border-radius:6px; background:color-mix(in srgb,var(--color-primary-hover) 45%,var(--color-surface)); color:var(--color-text-secondary); font:600 var(--font-size-body) 'Segoe UI',sans-serif; }
  .sync-status-chip.success { color:#16865C; border-color:color-mix(in srgb,#16865C 30%,var(--color-border)); background:color-mix(in srgb,#16865C 8%,var(--color-surface)); }
  .sync-status-chip.failed { color:#A95760; border-color:color-mix(in srgb,#A95760 32%,var(--color-border)); background:color-mix(in srgb,#A95760 8%,var(--color-surface)); }
  .sync-status-dot { width:.18rem; height:.18rem; border-radius:50%; background:var(--color-primary); box-shadow:0 0 0 .1rem color-mix(in srgb,var(--color-primary) 12%,transparent); }
  .sync-status-symbol { font-weight:800; }
  .sync-actions { display:flex; flex-wrap:wrap; gap:.4rem; align-items:center; }
  .management { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:.6rem; margin-top:1.46rem; }
  .management .plain-btn,.management .danger-btn { width:100%; min-width:0; height:1.55rem; padding:.2rem .25rem; line-height:1.15; white-space:normal; }
  .plain-btn,.danger-btn { border:1px solid var(--color-primary); border-radius:6px; padding:.28rem .5rem; background:var(--color-surface); color:var(--color-primary); cursor:pointer; font:600 var(--font-size-body) 'Segoe UI',sans-serif; }
  .danger-btn { color:#DC2626; border-color:#FCA5A5; }
  .http-warning { color:var(--color-text-secondary); font:500 var(--font-size-caption)/1.5 'Segoe UI',sans-serif; }
  .http-warning { grid-column:1/-1; color:#B45309; }
  .sync-history-latest { display:grid; grid-template-columns:minmax(0,1fr) auto auto; align-items:center; gap:.6rem; min-height:1.36rem; margin-top:.75rem; padding-top:.6rem; border-top:1px solid var(--color-border); color:var(--color-text-secondary); font:500 var(--font-size-body)/1.45 'Segoe UI',sans-serif; }
  .sync-history-latest strong { color:#16865C; font-size:var(--font-size-body); font-weight:700; white-space:nowrap; }
  .history-details-btn { height:1.05rem; padding:0 .45rem; border:1px solid color-mix(in srgb,var(--color-primary) 28%,var(--color-border)); border-radius:5px; background:var(--color-surface); color:var(--color-primary); cursor:pointer; font:600 var(--font-size-body) 'Segoe UI',sans-serif; white-space:nowrap; }
  .history-failed { color:#DC2626; }
  .compact-action { display:inline-flex; align-items:center; justify-content:center; height:1.36rem; box-sizing:border-box; padding:0 .65rem; white-space:nowrap; }
  .save-status { display:flex; align-items:center; justify-content:center; min-width:1rem; }
  .status-spinner { width:.42rem; height:.42rem; box-sizing:border-box; border:2px solid var(--color-border); border-top-color:var(--color-primary); border-radius:50%; animation:sync-spin .7s linear infinite; }
  .status-success,.status-failed { display:flex; align-items:center; justify-content:center; width:.55rem; height:.55rem; border-radius:50%; color:#FFFFFF; font:700 var(--font-size-caption)/1 'Segoe UI',sans-serif; }
  .status-success { background:#16A34A; }
  .status-failed { background:#DC2626; }
  @keyframes sync-spin { to { transform:rotate(360deg); } }

  .sync-modal-backdrop {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    margin: 0;
    padding: 1.2rem;
    border: 0;
    background: rgba(8, 12, 24, 0.45);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    z-index: 1000;
  }

  .sync-modal-card {
    width: min(440px, 100%);
    max-height: calc(100vh - 2.4rem);
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-surface);
    box-shadow: 0 18px 50px rgba(8, 12, 24, 0.24);
    display: flex;
    flex-direction: column;
  }

  .sync-modal-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    padding: .85rem 1rem .7rem;
    border-bottom: 1px solid var(--color-border);
  }

  .sync-modal-header h2 { margin:0; color:var(--color-text-primary); font:700 var(--font-size-dialog-title) 'Segoe UI Variable Display','Segoe UI',sans-serif; }
  .sync-modal-header p { margin:.18rem 0 0; color:var(--color-text-secondary); font:500 var(--font-size-body)/1.45 'Segoe UI',sans-serif; }
  .sync-modal-close { border:0; background:transparent; color:var(--color-text-tertiary); cursor:pointer; font:400 1rem/1 'Segoe UI',sans-serif; padding:0 .1rem; }
  .sync-modal-close:hover { color:var(--color-text-primary); }
  .sync-modal-body { padding:.85rem 1rem 1rem; overflow-y:auto; }

  .management-dialog-card { width:min(380px,100%); height:min(292px,calc(100vh - 2.4rem)); display:flex; flex-direction:column; border:1px solid var(--color-border); border-radius:10px; background:var(--color-surface); box-shadow:0 18px 50px rgba(8,12,24,.28); overflow:hidden; }
  .management-dialog-card.compact-management-dialog { width:min(340px,100%); height:min(220px,calc(100vh - 2.4rem)); }
  .management-dialog-header { display:flex; align-items:flex-start; justify-content:space-between; gap:.8rem; padding:.8rem .9rem .65rem; border-bottom:1px solid var(--color-border); }
  .management-dialog-header h2 { margin:0; color:var(--color-text-primary); font:700 var(--font-size-dialog-title) 'Segoe UI Variable Display','Segoe UI',sans-serif; }
  .management-dialog-header p { margin:.2rem 0 0; color:var(--color-text-secondary); font:500 var(--font-size-body)/1.45 'Segoe UI',sans-serif; }
  .management-dialog-body { flex:1; min-height:0; display:flex; flex-direction:column; justify-content:center; gap:.55rem; padding:.75rem .9rem; overflow:hidden; }
  .password-field { display:flex; flex-direction:column; gap:.22rem; color:var(--color-text-secondary); font:600 var(--font-size-body) 'Segoe UI',sans-serif; }
  .password-error { margin:0; color:#A95760; font:600 var(--font-size-caption)/1.35 'Segoe UI',sans-serif; }
  .management-dialog-note { margin:0; color:var(--color-text-secondary); font:500 var(--font-size-body)/1.65 'Segoe UI',sans-serif; }
  .management-dialog-warning { margin:0; color:#A95760; font:600 var(--font-size-caption)/1.5 'Segoe UI',sans-serif; }
  .management-dialog-actions { display:flex; justify-content:flex-end; gap:.4rem; padding:.65rem .9rem .8rem; }
  .management-dialog-actions .compact-action { min-width:2.65rem; }
  .history-dialog-card { width:min(380px,100%); height:min(292px,calc(100vh - 2.4rem)); display:flex; flex-direction:column; border:1px solid var(--color-border); border-radius:10px; background:var(--color-surface); box-shadow:0 18px 50px rgba(8,12,24,.28); overflow:hidden; }
  .history-dialog-list { flex:1; min-height:0; overflow-y:auto; padding:.35rem .9rem; }
  .history-dialog-list div { display:flex; justify-content:space-between; gap:.6rem; padding:.38rem 0; border-bottom:1px solid color-mix(in srgb,var(--color-border) 65%,transparent); color:var(--color-text-secondary); font:500 var(--font-size-body)/1.45 'Segoe UI',sans-serif; }
  .history-dialog-list div:last-child { border-bottom:0; }

  .confirm-backdrop {
    position:fixed;
    inset:0;
    width:100vw;
    height:100vh;
    margin:0;
    padding:1.2rem;
    border:0;
    background:rgba(8,12,24,.58);
    display:flex;
    align-items:center;
    justify-content:center;
    box-sizing:border-box;
    z-index:1100;
  }
  .confirm-card { width:min(320px,100%); padding:1rem; border:1px solid #FCA5A5; border-radius:10px; background:var(--color-surface); box-shadow:0 18px 50px rgba(8,12,24,.28); }
  .confirm-card h2 { margin:0; color:var(--color-text-primary); font:700 var(--font-size-dialog-title) 'Segoe UI Variable Display','Segoe UI',sans-serif; }
  .confirm-card p { margin:.45rem 0 .8rem; color:var(--color-text-secondary); font:500 var(--font-size-body)/1.55 'Segoe UI',sans-serif; }
  .confirm-actions { display:flex; justify-content:flex-end; gap:.45rem; }
  .danger-confirm { border:0; border-radius:6px; padding:.3rem .6rem; background:#DC2626; color:#FFFFFF; cursor:pointer; font:600 var(--font-size-body) 'Segoe UI',sans-serif; }

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
    font-size: var(--font-size-body);
    font-weight: 600;
    color: var(--color-text-primary);
  }

  /* 关于页面样式 */
  .about-section {
    @apply flex flex-col;
  }

  .about-desc {
    font-family: 'Segoe UI', sans-serif;
    font-size: var(--font-size-body);
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
    font-size: var(--font-size-body);
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .metadata-value {
    font-family: 'JetBrains Mono', monospace;
    font-size: var(--font-size-body);
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .link-btn {
    @apply bg-transparent border-none p-0 cursor-pointer font-bold transition-all;
    font-family: 'Segoe UI', sans-serif;
    font-size: var(--font-size-body);
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
    font-size: var(--font-size-body);
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
