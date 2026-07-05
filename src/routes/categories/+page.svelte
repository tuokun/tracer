<script lang="ts">
  import { onMount } from 'svelte';
  import { getCategories, saveCategory, deleteCategory } from '$lib/api/commands';
  import { CATEGORY_COLORS } from '$lib/utils/colors';
  import type { CategoryItem } from '$lib/api/types';

  let cats = $state<CategoryItem[]>([]);
  let showForm = $state(false);
  let editingId = $state<number | null>(null);
  let formName = $state('');
  let formColor = $state('#5048E5');
  let formRules = $state('');

  const colors = CATEGORY_COLORS;

  onMount(() => loadCats());

  async function loadCats() { cats = await getCategories(); }

  function startEdit(cat?: CategoryItem) {
    if (cat) {
      editingId = cat.id; formName = cat.name; formColor = cat.color ?? '#5048E5'; formRules = cat.rules ?? '';
    } else {
      editingId = null; formName = ''; formColor = '#5048E5'; formRules = '';
    }
    showForm = true;
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!formName.trim()) return;
    await saveCategory(formName.trim(), formColor, formRules || undefined, editingId ?? undefined);
    showForm = false;
    await loadCats();
  }

  async function remove(id: number, name: string) {
    if (!confirm(`删除分类「${name}」？关联应用将变为未分类。`)) return;
    await deleteCategory(id);
    await loadCats();
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h1 class="page-title">分类管理</h1>
      <p class="page-desc">管理和配置应用分类</p>
    </div>
    <button class="add-btn" onclick={() => startEdit()}>+ 新建</button>
  </div>

  {#if showForm}
    <form class="card form-card" onsubmit={save}>
      <div class="form-row">
        <label class="form-label">名称</label>
        <input class="form-input" type="text" bind:value={formName} placeholder="分类名称" required />
      </div>
      <div class="form-row">
        <label class="form-label">颜色</label>
        <div class="color-picker">
          {#each colors as c}
            <button type="button" class="color-swatch" class:selected={formColor === c} style="background:{c}" onclick={() => formColor = c}></button>
          {/each}
        </div>
      </div>
      <div class="form-row">
        <label class="form-label">规则</label>
        <input class="form-input" type="text" bind:value={formRules} placeholder="glob 通配符，如 chrome*" />
      </div>
      <div class="form-actions">
        <button type="button" class="cancel-btn" onclick={() => showForm = false}>取消</button>
        <button type="submit" class="save-btn">保存</button>
      </div>
    </form>
  {/if}

  {#if cats.length === 0 && !showForm}
    <div class="empty-state">
      <span style="font-family:'Segoe UI',sans-serif;font-size:0.5rem;color:#9A92C8">暂无分类，点击右上角新建</span>
    </div>
  {:else}
    <div class="cat-grid">
      {#each cats as cat}
        <div class="cat-card">
          <div class="cat-head" style="background:{cat.color ?? '#5048E5'}">
            <span class="cat-name">{cat.name}</span>
          </div>
          <div class="cat-body">
            {#if cat.rules}
              <div class="cat-rules-label">规则</div>
              <div class="cat-rules">{cat.rules}</div>
            {:else}
              <span style="font-family:'Segoe UI',sans-serif;font-size:0.4rem;color:#9A92C8">手动指派</span>
            {/if}
          </div>
          <div class="cat-actions">
            <button class="cat-action-btn" onclick={() => startEdit(cat)}>编辑</button>
            <button class="cat-action-btn cat-action-danger" onclick={() => remove(cat.id, cat.name)}>删除</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { max-width: 800px; }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

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
  }

  .add-btn {
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    background: theme('colors.primary.DEFAULT');
    color: #FFFFFF;
    border: none;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .add-btn:hover { opacity: 0.9; }

  .form-card { padding: 1rem; margin-bottom: 1rem; }

  .form-row { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 0.75rem; }

  .form-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    color: theme('colors.text.primary');
    width: 40px;
    flex-shrink: 0;
  }

  .form-input {
    flex: 1;
    padding: 0.375rem 0.5rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    color: theme('colors.text.primary');
    outline: none;
  }

  .form-input:focus { border-color: theme('colors.primary.DEFAULT'); }

  .color-picker { display: flex; gap: 0.375rem; }

  .color-swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }

  .color-swatch.selected { border-color: theme('colors.text.primary'); }

  .form-actions { display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.5rem; }

  .cancel-btn {
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    border: 1px solid theme('colors.border');
    background: #FFFFFF;
    color: theme('colors.text.secondary');
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    cursor: pointer;
  }

  .save-btn {
    padding: 0.375rem 0.75rem;
    border-radius: 4px;
    border: none;
    background: theme('colors.primary.DEFAULT');
    color: #FFFFFF;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .empty-state {
    background: #FFFFFF;
    border: 1px solid theme('colors.border');
    border-radius: 4px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.05);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 3rem 0;
  }

  .cat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 0.75rem; }

  .cat-card {
    background: #FFFFFF;
    border: 1px solid theme('colors.border');
    border-radius: 4px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.05);
    overflow: hidden;
  }

  .cat-head {
    padding: 0.5rem 0.75rem;
  }

  .cat-name {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 600;
    font-size: 0.65rem;
    color: #FFFFFF;
  }

  .cat-body { padding: 0.5rem 0.75rem; min-height: 32px; }

  .cat-rules-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: theme('colors.text.tertiary');
    margin-bottom: 2px;
  }

  .cat-rules {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.4rem;
    color: theme('colors.text.secondary');
  }

  .cat-actions {
    display: flex;
    border-top: 1px solid theme('colors.heatmap.bg');
  }

  .cat-action-btn {
    flex: 1;
    padding: 0.375rem;
    border: none;
    background: none;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.4rem;
    color: theme('colors.text.secondary');
    cursor: pointer;
  }

  .cat-action-btn:hover { background: theme('colors.primary.hover'); color: theme('colors.primary.DEFAULT'); }

  .cat-action-danger:hover { background: rgba(229,57,53,0.08); color: #E53935; }
</style>
