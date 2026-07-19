<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getConfigValue, notifyFrontendReady } from '$lib/api/commands';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Titlebar from '$lib/components/Titlebar.svelte';

  let currentTheme = 'system';

  onMount(() => {
    let cancelled = false;

    currentTheme = getStoredTheme();
    applyTheme(currentTheme);
    notifyReadyAfterPaint();

    getConfigValue('theme')
      .then((theme) => {
        if (!cancelled && theme && theme !== currentTheme) {
          currentTheme = theme;
          applyTheme(currentTheme);
        }
      })
      .catch((e) => {
        console.error("加载主题配置失败:", e);
      });

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
      cancelled = true;
      window.removeEventListener('theme-changed', handleThemeChange as EventListener);
      mediaQuery.removeEventListener('change', sysThemeHandler);
    };
  });

  const themeVars = {
    light: { bg: '#F0F0EC', surface: '#FFFFFF', sidebar: '#F8F6FE', border: '#E0DCF0' },
    pure: { bg: '#FFFFFF', surface: '#FFFFFF', sidebar: '#FFFFFF', border: '#F0F0F5' },
    dark: { bg: '#0F172A', surface: '#1E293B', sidebar: '#0F172A', border: '#334155' },
    ocean: { bg: '#0B192C', surface: '#1E3E62', sidebar: '#000000', border: '#1E3E62' },
    forest: { bg: '#064E3B', surface: '#065F46', sidebar: '#022C22', border: '#047857' },
  };

  function getStoredTheme() {
    try {
      return localStorage.getItem('tracer-theme') || 'system';
    } catch (e) {
      return 'system';
    }
  }

  function notifyReadyAfterPaint() {
    requestAnimationFrame(() => {
      notifyFrontendReady().catch(console.error);
    });
  }

  function applyTheme(themeName: string) {
    try {
      localStorage.setItem('tracer-theme', themeName);
    } catch (e) {}

    const root = document.documentElement;
    // 移除所有可能的主题样式类
    root.classList.remove('theme-light', 'theme-pure', 'theme-dark', 'theme-ocean', 'theme-forest');
    
    let resolvedTheme = themeName;
    if (themeName === 'system') {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      resolvedTheme = isDark ? 'dark' : 'light';
    }

    const vars = themeVars[resolvedTheme as keyof typeof themeVars] ?? themeVars.light;
    root.style.setProperty('--color-bg', vars.bg);
    root.style.setProperty('--color-surface', vars.surface);
    root.style.setProperty('--color-sidebar', vars.sidebar);
    root.style.setProperty('--color-border', vars.border);
    
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
    background: var(--color-bg);
  }

  .app-shell {
    @apply flex flex-1 h-full overflow-hidden;
  }

  .main-content {
    @apply flex-1 overflow-y-auto pt-10 pr-4 pb-4 pl-4;
    background: var(--color-surface);
  }
</style>
