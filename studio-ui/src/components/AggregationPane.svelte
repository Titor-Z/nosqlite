<script>
  import {
    app,
    addStage,
    removeStage,
    moveStage,
    runAggregation,
  } from '../lib/store.svelte.js';
  import TableView from './TableView.svelte';
  import JsonView from './JsonView.svelte';

  let view = $state('table');

  const SUPPORTED = ['$match', '$sort', '$skip', '$limit', '$group', '$project', '$addFields', '$set'];
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center gap-2">
    <span class="label">Aggregation Pipeline</span>
    <span class="tag tabular">{app.pipeline.length} stages</span>
    <button class="btn btn-ghost ml-auto" onclick={addStage}>+ Add Stage</button>
    <button class="btn btn-primary" onclick={runAggregation} disabled={app.busy || !app.pipeline.length}>
      Run
    </button>
  </div>

  <p class="text-[10px] text-dim">
    支持阶段:{SUPPORTED.join(' / ')}(`$group` 累加器仅 `$sum` / `$count`)。
  </p>

  <div class="space-y-2">
    {#each app.pipeline as stage, i}
      <div class="panel-flat">
        <div class="flex items-center gap-2 border-b border-line px-3 py-2">
          <span class="text-[10px] tabular text-dim">STAGE {i + 1}</span>
          <div class="ml-auto flex gap-1">
            <button class="btn btn-ghost" onclick={() => moveStage(i, -1)} disabled={i === 0}>↑</button>
            <button
              class="btn btn-ghost"
              onclick={() => moveStage(i, 1)}
              disabled={i === app.pipeline.length - 1}
            >
              ↓
            </button>
            <button class="btn btn-danger" onclick={() => removeStage(i)}>✕</button>
          </div>
        </div>
        <textarea class="field h-24 w-full border-0" bind:value={stage.text} spellcheck="false"
        ></textarea>
        {#if app.aggregation.partials[i]}
          {@const partial = app.aggregation.partials[i]}
          <div class="border-t border-line px-3 py-2 text-[10px]">
            {#if partial.error}
              <span class="text-danger">{partial.error}</span>
            {:else}
              <span class="text-dim">预览:</span>
              <span class="tabular text-cyan">{partial.count}</span>
              <span class="text-dim">条</span>
              {#if partial.docs.length}
                <pre class="json-block mt-1 max-h-32 overflow-auto p-2"
                  >{JSON.stringify(partial.docs, null, 2)}</pre
                >
              {/if}
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  {#if app.aggregation.docs.length || app.aggregation.partials.length}
    <div class="flex items-center gap-2 border-y border-line py-2">
      <span class="label">Output</span>
      <span class="tabular text-[11px] text-cyan">{app.aggregation.docs.length} docs</span>
      <div class="ml-auto flex border border-line">
        <button
          class="px-3 py-1 text-[10px] uppercase tracking-[0.14em] {view === 'table'
            ? 'bg-cyan/15 text-cyan'
            : 'text-dim hover:text-text'}"
          onclick={() => (view = 'table')}>Table</button
        >
        <button
          class="border-l border-line px-3 py-1 text-[10px] uppercase tracking-[0.14em] {view ===
          'json'
            ? 'bg-cyan/15 text-cyan'
            : 'text-dim hover:text-text'}"
          onclick={() => (view = 'json')}>JSON</button
        >
      </div>
    </div>

    {#if app.aggregation.docs.length === 0}
      <div class="panel px-6 py-10 text-center text-[12px] text-dim">管道没有输出文档。</div>
    {:else if view === 'table'}
      <TableView docs={app.aggregation.docs} readonly />
    {:else}
      <JsonView docs={app.aggregation.docs} readonly />
    {/if}
  {/if}
</div>
