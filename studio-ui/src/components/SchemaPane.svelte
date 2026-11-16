<script>
  import { app, runSchema } from '../lib/store.svelte.js';

  const COLORS = {
    string: 'bg-mint',
    number: 'bg-amber',
    boolean: 'bg-cyan',
    object: 'bg-magenta',
    array: 'bg-cyan',
    null: 'bg-dim',
    undefined: 'bg-dim',
  };

  let sampleSize = $state(200);

  function analyze() {
    runSchema(sampleSize);
  }

  function share(entry) {
    return app.schema.sampleCount ? (entry.seen / app.schema.sampleCount) * 100 : 0;
  }

  function types(entry) {
    return Object.entries(entry.types).sort((a, b) => b[1] - a[1]);
  }
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center gap-2">
    <span class="label">Schema Analysis</span>
    <select class="field w-28" bind:value={sampleSize}>
      {#each [100, 200, 500, 1000] as n}
        <option value={n}>{n} docs</option>
      {/each}
    </select>
    <button class="btn btn-primary" onclick={analyze} disabled={app.busy}>Analyze</button>
    {#if app.schema.analyzed}
      <span class="ml-auto text-[11px] text-dim">
        采样 <span class="tabular text-text">{app.schema.sampleCount}</span> 条 ·
        <span class="tabular text-cyan">{app.schema.fields.length}</span> 个字段
      </span>
    {/if}
  </div>

  {#if !app.schema.analyzed}
    <div class="panel px-6 py-14 text-center text-[12px] text-dim">
      对当前集合采样,统计字段出现率与类型分布。采样只读取前 N 条文档。
    </div>
  {:else if app.schema.fields.length === 0}
    <div class="panel px-6 py-14 text-center text-[12px] text-dim">集合为空,没有可分析的字段。</div>
  {:else}
    <div class="space-y-1">
      {#each app.schema.fields as field}
        <div class="panel-flat px-3 py-2">
          <div class="flex items-center gap-3">
            <code class="min-w-0 flex-1 truncate text-[12px] text-text">{field.path}</code>
            <span class="tabular text-[11px] text-cyan">{share(field).toFixed(0)}%</span>
          </div>
          <div class="mt-2 flex h-1.5 w-full overflow-hidden bg-void">
            {#each types(field) as [type, count]}
              <div
                class="{COLORS[type] ?? 'bg-dim'} h-full"
                style="width: {(count / field.seen) * 100}%"
                title="{type} {count}"
              ></div>
            {/each}
          </div>
          <div class="mt-1 flex flex-wrap gap-2 text-[10px] text-dim">
            {#each types(field) as [type, count]}
              <span><span class="text-text">{type}</span> ×{count}</span>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
