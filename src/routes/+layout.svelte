<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getConfigValue } from '$lib/api/commands';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Titlebar from '$lib/components/Titlebar.svelte';

  let currentTheme = 'system';

  onMount(async () => {
    try {
      const theme = await getConfigValue('theme');
      if (theme) {
        currentTheme = theme;
      }
      applyTheme(currentTheme);
    } catch (e) {
      console.error("加载主题配置失败:", e);
      applyTheme('system');
    }

    // 监听全局主题变更事件
    const handleThemeChange = (e: CustomEvent<string>) => {
      currentTheme = e.detail;
      applyTheme(currentTheme);
    };

    // 监听系统主题变化
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const sysThemeHandler = () => {
      if (currentTheme === 'system') {
        applyTheme('system');
      }
    };

    window.addEventListener('theme-changed', handleThemeChange as EventListener);
    mediaQuery.addEventListener('change', sysThemeHandler);

    return () => {
      window.removeEventListener('theme-changed', handleThemeChange as EventListener);
      mediaQuery.removeEventListener('change', sysThemeHandler);
    };
  });

  function applyTheme(themeName: string) {
    const root = document.documentElement;
    // 移除所有可能的主题样式类
    root.classList.remove('theme-light', 'theme-pure', 'theme-dark', 'theme-ocean', 'theme-forest');
    
    let resolvedTheme = themeName;
    if (themeName === 'system') {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      resolvedTheme = isDark ? 'dark' : 'light';
    }
    
    if (resolvedTheme !== 'light') {
      root.classList.add(`theme-${resolvedTheme}`);
    }
  }
</script>

<div class="window-shell">
  <Titlebar />
  <div class="app-shell">
    <Sidebar />
    <main class="main-content">
      <slot />
    </main>
  </div>
</div>

<style>
  .window-shell {
    @apply relative w-full h-full flex flex-col overflow-hidden;
  }

  .app-shell {
    @apply flex flex-1 h-full overflow-hidden;
  }

  .main-content {
    @apply flex-1 overflow-y-auto pt-10 pr-4 pb-4 pl-4;
  }
</style>
