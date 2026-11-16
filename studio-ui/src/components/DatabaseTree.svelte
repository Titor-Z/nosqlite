<script>
  import { app, selectCollection, refresh, openNewDoc } from '../lib/store.svelte.js';
</script>

<aside class="flex min-h-0 flex-col border-r border-line bg-panel/50">
  <div class="flex items-center gap-2 border-b border-line px-3 py-2.5">
    <span class="label">Databases</span>
    <span class="tag tabular">{app.collections.length}</span>
    <button class="ml-auto text-[11px] text-dim hover:text-cyan" title="刷新" onclick={refresh}>
      ⟳
    </button>
  </div>

  <div class="min-h-0 flex-1 overflow-auto py-1">
    {#if app.collections.length === 0}
      <div class="px-4 py-6 text-[11px] leading-relaxed text-dim">
        暂无集合。插入第一条文档时会自动创建。
      </div>
    {:else}
      {#each app.collections as name}
        <button
          class="coll-item text-[12px]"
          class:active={app.selected === name}
          onclick={() => selectCollection(name)}
        >
          <span class="min-w-0 truncate">{name}</span>
          <span class="tabular text-[10px] text-dim">
            {app.docCounts[name] ?? '—'}
          </span>
        </button>
      {/each}
    {/if}
  </div>

  <div class="border-t border-line p-3">
    <button class="btn w-full" onclick={openNewDoc}>+ 插入文档</button>
  </div>
</aside>
