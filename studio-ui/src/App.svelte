<script>
  import { onMount } from 'svelte';
  import {
    app,
    boot,
    importFile,
    runExplain,
    refresh,
    askDropCollection,
    openNewDoc,
  } from './lib/store.svelte.js';
  import TopBar from './components/TopBar.svelte';
  import DatabaseTree from './components/DatabaseTree.svelte';
  import Landing from './components/Landing.svelte';
  import DocumentsPane from './components/DocumentsPane.svelte';
  import IndexPane from './components/IndexPane.svelte';
  import ExplainPane from './components/ExplainPane.svelte';
  import SchemaPane from './components/SchemaPane.svelte';
  import AggregationPane from './components/AggregationPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import DocumentEditor from './components/DocumentEditor.svelte';
  import ConfirmModal from './components/ConfirmModal.svelte';
  import Toast from './components/Toast.svelte';

  let dragging = $state(false);
  let dragDepth = 0;

  const tabs = [
    ['documents', 'Documents'],
    ['indexes', 'Indexes'],
    ['explain', 'Explain'],
    ['schema', 'Schema'],
    ['aggregation', 'Aggregation'],
  ];

  onMount(boot);

  function onDragEnter(event) {
    event.preventDefault();
    dragDepth += 1;
    dragging = true;
  }
  function onDragOver(event) {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
  }
  function onDragLeave(event) {
    event.preventDefault();
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) dragging = false;
  }
  function onDrop(event) {
    event.preventDefault();
    dragDepth = 0;
    dragging = false;
    const file = event.dataTransfer?.files?.[0];
    if (file) importFile(file);
  }

  function selectTab(tab) {
    app.tab = tab;
    if (tab === 'explain' && app.selected) runExplain();
    if (tab === 'aggregation' && app.pipeline.length === 0) {
      app.pipeline = [{ text: '{\n  "$match": {}\n}' }];
    }
  }
</script>

<svelte:window
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
/>

<div class="flex h-dvh min-h-0 flex-col">
  <TopBar />

  {#if !app.opened}
    <Landing />
  {:else}
    <!-- 桌面:左树 + 工作区;窄屏:左树压到上方一条(限高),工作区占满 -->
    <div
      class="grid min-h-0 flex-1 grid-rows-[minmax(0,32vh)_minmax(0,1fr)] lg:grid-cols-[270px_minmax(0,1fr)] lg:grid-rows-1"
    >
      <div class="min-h-0 overflow-hidden border-b-2 border-ink lg:border-b-0">
        <DatabaseTree />
      </div>

      <main class="flex min-h-0 min-w-0 flex-col">
        <header class="flex items-center gap-3 border-b border-line bg-panel/40 px-4 py-2.5">
          <span class="text-[10px] tracking-[0.2em] text-dim">nodblite</span>
          <span class="text-dim">/</span>
          <span class="text-[13px] text-text">{app.selected ?? '—'}</span>
          <span class="ml-auto flex items-center gap-2">
            <button class="btn btn-ghost" onclick={refresh}>刷新</button>
            {#if app.selected}
              <button class="btn btn-danger" onclick={() => askDropCollection(app.selected)}>
                删除集合
              </button>
            {/if}
          </span>
        </header>

        <nav class="flex items-center gap-1 border-b border-line bg-panel/60 px-3">
          {#each tabs as [key, label]}
            <button class="tab" class:active={app.tab === key} onclick={() => selectTab(key)}>
              {label}
            </button>
          {/each}
        </nav>

        <div class="min-h-0 flex-1 overflow-hidden p-4">
          {#if !app.selected}
            <div class="panel px-6 py-16 text-center text-[12px] text-dim">
              <p>还没有集合。插入第一条文档时会自动创建。</p>
              <button class="btn btn-primary mt-4" onclick={openNewDoc}>+ Insert Document</button>
            </div>
          {:else if app.tab === 'documents'}
            <DocumentsPane />
          {:else}
            <div class="h-full overflow-auto">
              {#if app.tab === 'indexes'}
                <IndexPane />
              {:else if app.tab === 'explain'}
                <ExplainPane />
              {:else if app.tab === 'schema'}
                <SchemaPane />
              {:else}
                <AggregationPane />
              {/if}
            </div>
          {/if}
        </div>
      </main>
    </div>
  {/if}

  <StatusBar />
</div>

{#if dragging}
  <div
    class="pointer-events-none fixed inset-0 z-[80] flex items-center justify-center bg-void/80 backdrop-blur-[2px]"
  >
    <div
      class="panel shadow-hard-lg px-16 py-12 text-center"
    >
      <div class="mb-3 text-[11px] tracking-[0.35em] text-cyan glow">DROP SIGNAL DETECTED</div>
      <div class="text-2xl tracking-[0.2em] text-text">释放以载入 .nodb</div>
    </div>
  </div>
{/if}

<DocumentEditor />
<ConfirmModal />
<Toast />
