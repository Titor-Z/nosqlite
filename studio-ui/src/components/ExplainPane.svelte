<script>
  import { app, runExplain, runFind } from '../lib/store.svelte.js';

  let raw = $state(false);
</script>

<div class="space-y-3">
  <div class="flex items-center gap-2">
    <span class="label">Query Plan</span>
    {#if app.explain?.plan}
      <span class="tag tag-cyan">{app.explain.plan.stage}</span>
      {#if app.explain.plan.indexName}
        <span class="tag tag-amber">{app.explain.plan.indexName}</span>
      {/if}
    {/if}
    <div class="ml-auto flex gap-2">
      <button class="btn btn-ghost" onclick={() => (raw = !raw)} disabled={!app.explain}>
        {raw ? 'Plan View' : 'Raw JSON'}
      </button>
      <button class="btn btn-primary" onclick={runExplain} disabled={app.busy}>Explain</button>
    </div>
  </div>

  {#if !app.explain}
    <div class="panel px-6 py-14 text-center text-[12px] text-dim">
      点击 Explain 以查看当前 Filter / Sort 的执行计划。
    </div>
  {:else if raw}
    <pre class="json-block overflow-auto p-4 text-[12px]">{JSON.stringify(app.explain.plan, null, 2)}</pre>
  {:else}
    {@const plan = app.explain.plan}
    <div class="panel p-4">
      <div class="mb-4 text-[10px] tracking-[0.2em] text-dim">WINNING PLAN</div>
      <div class="flex flex-wrap items-center gap-3">
        <div class="border border-cyan/50 px-4 py-3">
          <div class="text-[10px] text-dim">STAGE</div>
          <div class="text-[14px] text-cyan">{plan.stage}</div>
        </div>
        {#if plan.indexName}
          <span class="text-dim">→</span>
          <div class="border border-amber/50 px-4 py-3">
            <div class="text-[10px] text-dim">INDEX</div>
            <div class="text-[13px] text-amber">{plan.indexName}</div>
            {#if plan.field}<div class="text-[10px] text-dim">{plan.field}</div>{/if}
          </div>
        {/if}
        {#if plan.bounds}
          <span class="text-dim">→</span>
          <div class="border border-line px-4 py-3">
            <div class="text-[10px] text-dim">BOUNDS</div>
            <div class="text-[12px] text-text">{plan.bounds}</div>
          </div>
        {/if}
        <span class="text-dim">→</span>
        <div class="border border-line px-4 py-3">
          <div class="text-[10px] text-dim">FETCH</div>
          <div class="text-[12px] text-text">filter applied</div>
        </div>
      </div>

      <div class="mt-4 flex flex-wrap gap-2 text-[10px]">
        {#if plan.sortedByIndex}<span class="tag tag-magenta">SORTED BY INDEX</span>{/if}
        {#if plan.filterApplied}<span class="tag">FILTER APPLIED</span>{/if}
        <span class="tag">COLLECTION {plan.collection ?? app.selected ?? '—'}</span>
      </div>
    </div>

    <div class="grid grid-cols-2 gap-px bg-line lg:grid-cols-4">
      <div class="bg-panel px-4 py-3">
        <div class="label">Returned</div>
        <div class="tabular text-lg text-cyan">{app.explain.metrics.returned}</div>
      </div>
      <div class="bg-panel px-4 py-3">
        <div class="label">Matched</div>
        <div class="tabular text-lg text-text">{app.explain.metrics.total}</div>
      </div>
      <div class="bg-panel px-4 py-3">
        <div class="label">Elapsed</div>
        <div class="tabular text-lg text-amber">{app.explain.metrics.elapsed.toFixed(2)} ms</div>
      </div>
      <div class="bg-panel px-4 py-3">
        <div class="label">Skip / Limit</div>
        <div class="tabular text-lg text-text">
          {app.explain.metrics.skip} / {app.explain.metrics.limit}
        </div>
      </div>
    </div>

    <p class="text-[10px] text-dim">
      CLIENT MEASURED:返回条数与耗时由前端对同一查询实测;引擎不提供 docsExamined / keysExamined 统计。
    </p>
    <button class="btn btn-ghost" onclick={runFind}>按此条件重新查询</button>
  {/if}
</div>
