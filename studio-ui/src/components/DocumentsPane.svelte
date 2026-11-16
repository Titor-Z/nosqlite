<script>
  import { app } from '../lib/store.svelte.js';
  import QueryBar from './QueryBar.svelte';
  import ResultsToolbar from './ResultsToolbar.svelte';
  import TableView from './TableView.svelte';
  import JsonView from './JsonView.svelte';
</script>

<!-- 整列撑满可用高度:结果区自己滚动,横向滚动条常驻在视窗内 -->
<div class="flex h-full min-h-0 flex-col gap-3">
  <QueryBar />
  <ResultsToolbar />

  <div class="min-h-0 flex-1">
    {#if app.result.docs.length === 0}
      <div class="panel px-6 py-14 text-center text-[12px] text-dim">
        没有匹配的文档。调整 Filter 后点 Find,或用 Add Data 插入第一条。
      </div>
    {:else if app.view === 'table'}
      <TableView docs={app.result.docs} />
    {:else}
      <JsonView docs={app.result.docs} />
    {/if}
  </div>
</div>
