<script lang="ts">
  import { onMount } from 'svelte';
  import { getCategories, saveCategory, deleteCategory } from '$lib/api/commands';
  import { CATEGORY_COLORS } from '$lib/utils/colors';
  import type { CategoryItem } from '$lib/api/types';

  let cats = $state<CategoryItem[]>([]);
  let showForm = $state(false);
  let editingId = $state<number | null>(null);
  let formName = $state('');
  const DEFAULT_CAT_COLOR: string = CATEGORY_COLORS[0];
  let formColor = $state<string>(DEFAULT_CAT_COLOR);
  let formRules = $state('');

  const colors = CATEGORY_COLORS;

  onMount(() => loadCats());

  async function loadCats() { cats = await getCategories(); }

  function startEdit(cat?: CategoryItem) {
    if (cat) {
      editingId = cat.id; formName = cat.name; formColor = cat.color ?? DEFAULT_CAT_COLOR; formRules = cat.rules ?? '';
    } else {
      editingId = null; formName = ''; formColor = DEFAULT_CAT_COLOR; formRules = '';
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
        <!-- 注入 --cat-color 自定义变量 -->
        <div class="cat-card" style="--cat-color: {cat.color ?? DEFAULT_CAT_COLOR}">
          <div class="cat-color-bar"></div>
          <div class="cat-head">
            <!-- 矢量 SVG 标签图标，继承分类色彩 -->
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5" class="cat-tag-icon" style="color: var(--cat-color)">
              <path stroke-linecap="round" stroke-linejoin="round" d="M7 7h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <span class="cat-name">{cat.name}</span>
          </div>
          <div class="cat-body">
            {#if cat.rules}
              <div class="cat-rules-label">规则</div>
              <div class="cat-rules">{cat.rules}</div>
            {:else}
              <div class="cat-rules-label" style="visibility: hidden;">占位</div>
              <span class="cat-manual-badge">手动指派</span>
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
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.2s;
  }

  .color-swatch:hover { transform: scale(1.1); }
  .color-swatch.selected { border-color: theme('colors.text.primary'); transform: scale(1.15); }

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
    position: relative;
    border: 1px solid color-mix(in srgb, var(--cat-color, #5048E5) 12%, transparent);
    background: linear-gradient(135deg, color-mix(in srgb, var(--cat-color, #5048E5) 6%, transparent) 0%, color-mix(in srgb, var(--cat-color, #5048E5) 1.5%, transparent) 100%);
    border-radius: 8px;
    overflow: hidden;
    transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1), box-shadow 0.25s, border-color 0.25s;
    box-shadow: 0 4px 12px rgba(80,72,229,0.01), 0 1px 2px rgba(0,0,0,0.01);
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .cat-card:hover {
    transform: translateY(-3px);
    box-shadow: 0 8px 20px color-mix(in srgb, var(--cat-color, #5048E5) 8%, transparent);
    border-color: color-mix(in srgb, var(--cat-color, #5048E5) 25%, transparent);
  }

  /* 顶部色彩窄横条 */
  .cat-color-bar {
    height: 4px;
    width: 100%;
    background: var(--cat-color, #5048E5);
  }

  .cat-head {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.5rem 0.75rem 0.25rem 0.75rem;
  }

  .cat-tag-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }

  .cat-name {
    font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif;
    font-weight: 600;
    font-size: 0.8rem;
    color: theme('colors.text.primary');
  }

  .cat-body { padding: 0.25rem 0.75rem 0.6rem 0.75rem; flex: 1; }

  .cat-rules-label {
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.62rem;
    color: theme('colors.text.tertiary');
    margin-bottom: 3px;
  }

  /* 代码徽章自适应配色 */
  .cat-rules {
    display: inline-block;
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.62rem;
    font-weight: 600;
    color: var(--cat-color, #5048E5);
    background: color-mix(in srgb, var(--cat-color, #5048E5) 7%, transparent);
    padding: 0.08rem 0.3rem;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--cat-color, #5048E5) 12%, transparent);
  }

  /* 手动指派徽章 */
  .cat-manual-badge {
    display: inline-block;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.62rem;
    color: theme('colors.text.tertiary');
    background: #FAF9FD;
    padding: 0.08rem 0.3rem;
    border-radius: 4px;
    border: 1px solid #ECE9F5;
  }

  .cat-actions {
    display: flex;
    margin-top: auto;
    border-top: 1px solid color-mix(in srgb, var(--cat-color, #5048E5) 8%, transparent);
  }

  .cat-action-btn {
    flex: 1;
    width: 50%;
    box-sizing: border-box;
    padding: 0.375rem;
    border: none;
    background: none;
    font-family: 'Segoe UI', sans-serif;
    font-size: 0.6rem;
    font-weight: 600;
    color: color-mix(in srgb, var(--cat-color, #5048E5) 70%, #666);
    cursor: pointer;
    transition: background 0.2s, color 0.2s;
  }

  .cat-action-btn:hover {
    background: color-mix(in srgb, var(--cat-color, #5048E5) 8%, transparent);
    color: var(--cat-color, #5048E5);
  }

  .cat-action-danger:hover {
    background: rgba(229, 57, 53, 0.06);
    color: #E53935;
  }
</style>
