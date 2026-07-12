<script lang="ts">
  import { onMount } from 'svelte';

  let appWindow: any = null;
  let isMaximized = false;

  onMount(async () => {
    try {
      // 动态导入以防 SSR 报错
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      appWindow = getCurrentWindow();
      
      // 初始化最大化状态
      isMaximized = await appWindow.isMaximized();

      // 监听窗口大小变化以更新最大化状态
      const unlisten = await appWindow.onResized(async () => {
        isMaximized = await appWindow.isMaximized();
      });

      return () => {
        unlisten();
      };
    } catch (e) {
      console.warn('Tauri API 未加载，可能不在桌面环境中运行:', e);
    }
  });

  function handleMinimize() {
    if (appWindow) appWindow.minimize();
  }

  function handleToggleMaximize() {
    if (appWindow) {
      appWindow.toggleMaximize();
    }
  }

  function handleClose() {
    if (appWindow) appWindow.close();
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <!-- 左侧留空，直接展示下方的 Sidebar Logo -->
  <div class="drag-placeholder" data-tauri-drag-region></div>

  <!-- 右侧窗口控制按钮 -->
  <div class="window-controls">
    <button class="control-btn min-btn" onclick={handleMinimize} title="最小化">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" class="control-icon">
        <path fill="currentColor" d="M128 544h768a32 32 0 1 0 0-64H128a32 32 0 0 0 0 64z"/>
      </svg>
    </button>
    <button class="control-btn max-btn" onclick={handleToggleMaximize} title={isMaximized ? "还原" : "最大化"}>
      {#if isMaximized}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" class="control-icon">
          <path fill="currentColor" d="M768 160H256a96 96 0 0 0-96 96v512a96 96 0 0 0 96 96h512a96 96 0 0 0 96-96V256a96 96 0 0 0-96-96zm32 608a32 32 0 0 1-32 32H256a32 32 0 0 1-32-32v-80h576v80zm0-144H224V320h576v304zm0-368H224v-40a32 32 0 0 1 32-32h512a32 32 0 0 1 32 32v40z"/>
        </svg>
      {:else}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" class="control-icon">
          <path fill="currentColor" d="M768 128H256a128 128 0 0 0-128 128v512a128 128 0 0 0 128 128h512a128 128 0 0 0 128-128V256a128 128 0 0 0-128-128zm64 640a64 64 0 0 1-64 64H256a64 64 0 0 1-64-64V256a64 64 0 0 1 64-64h512a64 64 0 0 1 64 64v512z"/>
        </svg>
      {/if}
    </button>
    <button class="control-btn close-btn" onclick={handleClose} title="关闭">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" class="control-icon">
        <path fill="currentColor" d="M195.2 195.2a64 64 0 0 1 90.496 0L512 421.504l226.304-226.3a64 64 0 0 1 90.496 90.496L602.496 512l226.3 226.304a64 64 0 0 1-90.496 90.496L512 602.496 285.696 828.8a64 64 0 0 1-90.496-90.496L421.504 512l-226.3-226.304a64 64 0 0 1 0-90.496z"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    @apply fixed top-0 left-0 right-0 z-50 flex justify-between items-center select-none;
    height: 32px;
    background: linear-gradient(to right, 
      transparent 0px, 
      transparent 140px, 
      var(--color-surface) 140px, 
      var(--color-surface) 100%
    );
  }

  .drag-placeholder {
    @apply flex-1 h-full cursor-default;
  }

  .window-controls {
    @apply flex h-full items-stretch shrink-0;
  }

  .control-btn {
    @apply flex items-center justify-center border-none bg-transparent cursor-pointer transition-colors duration-150 text-text-primary;
    width: 46px;
    height: 100%;
    padding: 0;
  }

  .control-btn:focus {
    @apply outline-none;
  }

  .control-icon {
    width: 12px;
    height: 12px;
  }

  .control-btn:hover {
    @apply bg-primary-hover;
  }

  .close-btn:hover {
    background-color: #E81123 !important;
    color: #FFFFFF !important;
  }
</style>
