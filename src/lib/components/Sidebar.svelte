<script lang="ts">
  import { page } from '$app/stores';

  const nav = [
    { href: '/', label: '仪表盘' },
    { href: '/stats', label: '统计' },
    { href: '/apps', label: '应用' },
    { href: '/categories', label: '分类' },
    { href: '/settings', label: '设置' },
  ];
</script>

<aside class="sidebar">
  <div class="logo-area">
    <span class="logo-icon">
      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-4 h-4 text-primary">
        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
      </svg>
    </span>
    <span class="logo-text">Tracer</span>
  </div>

  <nav class="nav-list">
    {#each nav as item}
      <a
        href={item.href}
        class="nav-item"
        class:active={$page.url.pathname === item.href}
      >
        <span class="nav-icon">
          {#if item.href === '/'}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-full h-full">
              <path stroke-linecap="round" stroke-linejoin="round" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
            </svg>
          {:else if item.href === '/stats'}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-full h-full">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2m0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 002 2h2a2 2 0 002-2" />
            </svg>
          {:else if item.href === '/apps'}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-full h-full">
              <path stroke-linecap="round" stroke-linejoin="round" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
            </svg>
          {:else if item.href === '/categories'}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-full h-full">
              <path stroke-linecap="round" stroke-linejoin="round" d="M7 7h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
          {:else if item.href === '/settings'}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" class="w-full h-full">
              <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          {/if}
        </span>
        <span class="nav-label">{item.label}</span>
      </a>
    {/each}
  </nav>
</aside>

<style>
  .sidebar {
    @apply bg-sidebar border-r border-border flex flex-col h-screen overflow-hidden select-none;
    width: 140px;
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
    box-shadow: inset 3px 0 0 theme('colors.primary.DEFAULT');
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
</style>
