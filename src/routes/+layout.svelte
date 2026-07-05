<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
</script>

<div class="app-shell">
  <Sidebar />
  <main class="main-content">
    <slot />
  </main>
</div>

<style>
  .app-shell {
    @apply flex h-screen overflow-hidden;
  }

  .main-content {
    @apply flex-1 overflow-y-auto p-4;
  }
</style>
