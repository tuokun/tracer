<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { collapseSidebar } from '$lib/stores/sidebar.svelte';

  function onContentClick() {
    collapseSidebar();
  }
</script>

<div class="app-shell">
  <Sidebar />
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
  <main class="main-content" onclick={onContentClick}>
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
