<script>
  import { app, askDropIndex } from '../lib/store.svelte.js';
  import IndexBuilder from './IndexBuilder.svelte';

  let builderOpen = $state(false);
</script>

<div class="space-y-3">
  <div class="flex items-center gap-2">
    <span class="label">Indexes</span>
    <span class="tag tabular">{app.indexes.length}</span>
    <button class="btn btn-primary ml-auto" onclick={() => (builderOpen = true)}>Create Index</button>
  </div>

  <div class="overflow-auto border border-line">
    <table class="data-grid w-full">
      <thead class="bg-panel">
        <tr>
          <th>Name</th>
          <th>Fields</th>
          <th>Type</th>
          <th>Size</th>
          <th class="w-24"></th>
        </tr>
      </thead>
      <tbody>
        {#each app.indexes as index}
          <tr class="group">
            <td class="text-text">{index.name}</td>
            <td>
              <div class="flex flex-wrap items-center gap-1">
                {#each index.fields as field, i}
                  <span class="tag tag-cyan">{field} {index.directions?.[i] === -1 ? '↓' : '↑'}</span>
                {/each}
              </div>
            </td>
            <td>
              <span class="text-[11px] text-dim">
                {index.unique ? 'Unique' : index.sparse ? 'Sparse' : 'Regular'}
              </span>
            </td>
            <td class="text-dim">—</td>
            <td class="text-right">
              <button class="btn btn-danger" onclick={() => askDropIndex(index.name)}>Drop</button>
            </td>
          </tr>
        {:else}
          <tr>
            <td colspan="5" class="px-4 py-10 text-center text-[12px] text-dim">
              该集合暂无索引。等值与范围查询会自动选择可用索引。
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <p class="text-[10px] text-dim">Size 列显示 — 引擎不维护索引大小统计。</p>
</div>

{#if builderOpen}
  <IndexBuilder onClose={() => (builderOpen = false)} />
{/if}
