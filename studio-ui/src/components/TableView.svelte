<script>
  import {
    app,
    toggleSort,
    openEditDoc,
    duplicateDoc,
    askDeleteDoc,
  } from '../lib/store.svelte.js';

  let { docs = [], readonly = false } = $props();

  let expandedRow = $state({});
  let expandedCell = $state({});

  const columns = $derived.by(() => {
    const seen = new Set();
    const list = [];
    for (const doc of docs) {
      for (const key of Object.keys(doc)) {
        if (key === '_id' || seen.has(key)) continue;
        seen.add(key);
        list.push(key);
      }
    }
    return ['_id', ...list];
  });

  const currentSort = $derived.by(() => {
    try {
      return app.query.sort.trim() ? JSON.parse(app.query.sort) : {};
    } catch {
      return {};
    }
  });

  function cell(value) {
    if (value === null) return { text: 'null', cls: 'text-dim italic' };
    if (value === undefined) return { text: '', cls: 'text-dim' };
    if (Array.isArray(value)) return { text: `[${value.length}]`, cls: 'text-magenta' };
    if (typeof value === 'object') {
      const keys = Object.keys(value);
      if (keys.length === 1 && keys[0] === '$date') return { text: String(value.$date), cls: 'text-mint' };
      return { text: `{${keys.length}}`, cls: 'text-cyan' };
    }
    if (typeof value === 'string') return { text: value, cls: 'text-mint' };
    if (typeof value === 'number') return { text: String(value), cls: 'text-amber' };
    if (typeof value === 'boolean') return { text: String(value), cls: 'text-cyan' };
    return { text: String(value), cls: 'text-text' };
  }

  function isComplex(value) {
    return value !== null && typeof value === 'object';
  }

  function toggleCell(key) {
    expandedCell = { ...expandedCell, [key]: !expandedCell[key] };
  }

  function toggleRow(key) {
    expandedRow = { ...expandedRow, [key]: !expandedRow[key] };
  }

  function sortBy(field) {
    if (readonly) return;
    toggleSort(field);
  }
</script>

<div class="h-full min-h-0 overflow-auto border border-line">
  <table class="data-grid w-full">
    <thead class="sticky top-0 z-10 bg-panel">
      <tr>
        <th class="w-8"></th>
        {#each columns as col}
          <th>
            <button
              class="inline-flex items-center gap-1 {readonly
                ? 'cursor-default'
                : 'cursor-pointer hover:text-cyan'}"
              onclick={() => sortBy(col)}
              title={readonly ? col : `按 ${col} 排序`}
            >
              {col}
              {#if currentSort?.[col] === 1}
                <span class="text-cyan">↑</span>
              {:else if currentSort?.[col] === -1}
                <span class="text-cyan">↓</span>
              {/if}
            </button>
          </th>
        {/each}
        {#if !readonly}<th class="w-40">actions</th>{/if}
      </tr>
    </thead>
    <tbody>
      {#each docs as doc, i (doc._id ?? i)}
        {@const rowKey = String(doc._id ?? i)}
        <tr class="group">
          <td class="align-top">
            <button
              class="text-[10px] text-dim transition hover:text-cyan"
              onclick={() => toggleRow(rowKey)}
              title="展开文档"
            >
              {expandedRow[rowKey] ? '▾' : '▸'}
            </button>
          </td>
          {#each columns as col}
            {@const value = doc[col]}
            {@const c = cell(value)}
            {@const cellKey = `${rowKey}:${col}`}
            <td class="align-top">
              {#if isComplex(value)}
                <button
                  class="block max-w-[360px] cursor-pointer truncate text-left {c.cls} hover:underline"
                  onclick={() => toggleCell(cellKey)}
                >
                  {c.text}
                </button>
                {#if expandedCell[cellKey]}
                  <pre class="json-block mt-1 max-h-40 overflow-auto p-2 text-[11px]"
                    >{JSON.stringify(value, null, 2)}</pre
                  >
                {/if}
              {:else}
                <span class="block max-w-[360px] truncate {c.cls}"
                  >{col === '_id' ? String(value) : c.text}</span
                >
              {/if}
            </td>
          {/each}
          {#if !readonly}
            <td class="whitespace-nowrap align-top">
              <div class="flex gap-1 opacity-0 transition group-hover:opacity-100">
                <button class="btn btn-ghost" onclick={() => openEditDoc(doc)}>编辑</button>
                <button class="btn btn-ghost" onclick={() => duplicateDoc(doc)}>复制</button>
                <button class="btn btn-danger" onclick={() => askDeleteDoc(doc)}>删除</button>
              </div>
            </td>
          {/if}
        </tr>
        {#if expandedRow[rowKey]}
          <tr>
            <td colspan={columns.length + (readonly ? 1 : 2)} class="p-0">
              <pre class="json-block m-2 max-h-72 overflow-auto p-3 text-[11px]"
                >{JSON.stringify(doc, null, 2)}</pre
              >
            </td>
          </tr>
        {/if}
      {/each}
    </tbody>
  </table>
</div>
