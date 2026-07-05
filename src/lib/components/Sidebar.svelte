<script lang="ts">
  import { page } from '$app/stores';
  import { isSidebarCollapsed, toggleSidebar } from '$lib/stores/sidebar.svelte';

  const nav = [
    { href: '/', label: '仪表盘', icon: '◉' },
    { href: '/stats', label: '统计', icon: '⊟' },
    { href: '/apps', label: '应用', icon: '⊞' },
    { href: '/categories', label: '分类', icon: '◈' },
    { href: '/settings', label: '设置', icon: '⚙' },
  ];
</script>

<aside
  class="sidebar"
  class:collapsed={isSidebarCollapsed()}
>
  <!-- logo area -->
  <div class="logo-area">
    <span class="logo-icon">◈</span>
    <span class="logo-text" class:hidden={isSidebarCollapsed()}>Tracer</span>
  </div>

  <!-- navigation -->
  <nav class="nav-list">
    {#each nav as item}
      <a
        href={item.href}
        class="nav-item"
        class:active={$page.url.pathname === item.href}
      >
        <span class="nav-icon">{item.icon}</span>
        <span class="nav-label" class:hidden={isSidebarCollapsed()}>{item.label}</span>
      </a>
    {/each}
  </nav>

  <!-- collapse toggle -->
  <button class="toggle-btn" onclick={toggleSidebar}>
    <span class="toggle-icon">{isSidebarCollapsed() ? '▶' : '◀'}</span>
  </button>
</aside>

<style>
  .sidebar {
    @apply bg-sidebar border-r border-border flex flex-col h-screen overflow-hidden select-none;
    width: 140px;
    transition: width 0.2s ease;
  }

  .sidebar.collapsed {
    width: 40px;
  }

  .logo-area {
    @apply flex items-center h-12 px-2 gap-2 border-b border-border shrink-0;
  }

  .logo-icon {
    @apply text-primary text-lg shrink-0;
    width: 24px;
    text-align: center;
  }

  .logo-text {
    @apply font-display font-semibold text-text-primary whitespace-nowrap;
    font-size: 0.75rem;
  }

  .nav-list {
    @apply flex flex-col gap-0.5 px-1.5 py-3 flex-1;
  }

  .nav-item {
    @apply flex items-center h-8 px-1.5 gap-2 rounded no-underline transition-colors;
    @apply text-text-secondary hover:bg-primary-hover;
  }

  .nav-item.active {
    @apply bg-primary-hover text-primary;
    box-shadow: inset 3px 0 0 #5048E5;
  }

  .nav-icon {
    @apply shrink-0 text-center;
    width: 24px;
    font-size: 0.75rem;
  }

  .nav-label {
    @apply font-body font-semibold whitespace-nowrap;
    font-size: 0.65rem;
  }

  .toggle-btn {
    @apply flex items-center justify-center h-8 border-t border-border shrink-0 cursor-pointer;
    @apply text-text-tertiary hover:text-text-secondary hover:bg-primary-hover;
    background: none;
  }

  .toggle-icon {
    font-size: 0.5rem;
  }

  .hidden {
    display: none;
  }
</style>
