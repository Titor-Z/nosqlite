<script>
  import {
    openEditDoc,
    duplicateDoc,
    askDeleteDoc,
  } from '../lib/store.svelte.js';

  let { docs = [], readonly = false } = $props();

  let open = $state({});

  function keyOf(doc, index) {
    return String(doc._id ?? index);
  }

  function toggleAll(value) {
    const next = {};
    docs.forEach((doc, i) => {
      next[keyOf(doc, i)] = value;
    });
    open = next;
  }
</script>

<div class="h-full min-h-0 space-y-2 overflow-auto pr-1">
  <div class="flex items-center gap-2 text-[10px] text-dim">
    <button class="hover:text-cyan" onclick={() => toggleAll(true)}>展开全部</button>
    <span class="text-line">|</span>
    <button class="hover:text-cyan" onclick={() => toggleAll(false)}>折叠全部</button>
  </div>

  {#each docs as doc, i (doc._id ?? i)}
    {@const key = keyOf(doc, i)}
    <article class="panel-flat">
      <header class="flex items-center gap-2 border-b border-line px-3 py-2">
        <button
          class="text-[11px] text-dim transition hover:text-cyan"
          onclick={() => (open = { ...open, [key]: !open[key] })}
        >
          {open[key] ? '▾' : '▸'}
        </button>
        <span class="text-[10px] tabular text-dim">#{i + 1}</span>
        <code class="min-w-0 flex-1 truncate text-[11px] text-mint">{doc._id ?? '(no _id)'}</code>
        {#if !readonly}
          <button class="btn btn-ghost" onclick={() => openEditDoc(doc)}>编辑</button>
          <button class="btn btn-ghost" onclick={() => duplicateDoc(doc)}>复制</button>
          <button class="btn btn-danger" onclick={() => askDeleteDoc(doc)}>删除</button>
        {/if}
      </header>
      {#if open[key]}
        <pre class="json-block overflow-auto p-3 text-[12px]">{JSON.stringify(doc, null, 2)}</pre>
      {:else}
        <div class="flex flex-wrap gap-x-4 gap-y-1 px-3 py-2 text-[11px]">
          {#each Object.entries(doc) as [field, value]}
            {#if field !== '_id'}
              <span class="min-w-0 truncate">
                <span class="text-dim">{field}:</span>
                <span class="text-text">{typeof value === 'string' ? value : JSON.stringify(value)}</span>
              </span>
            {/if}
          {/each}
        </div>
      {/if}
    </article>
  {/each}
</div>
