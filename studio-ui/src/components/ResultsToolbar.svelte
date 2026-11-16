<script>
  import {
    app,
    runFind,
    openNewDoc,
    openBulkDocs,
    importCollectionFile,
    exportCollection,
  } from '../lib/store.svelte.js';

  let fileInput = $state(null);
  let menu = $state(null);

  const skip = $derived(Number(app.query.skip || 0) || 0);
  const start = $derived(app.result.docs.length ? skip + 1 : 0);
  const end = $derived(skip + app.result.docs.length);
  const hasPrev = $derived(skip > 0);
  const hasNext = $derived(end < app.result.total);

  function step(delta) {
    const limit = Number(app.query.limit || 20) || 20;
    const next = Math.max(0, skip + delta * limit);
    app.query.skip = next ? String(next) : '';
    runFind();
  }

  function pickFile(event) {
    const file = event.target.files?.[0];
    if (file) importCollectionFile(file);
    event.target.value = '';
  }

  function chooseExport(format) {
    exportCollection(format);
  }
</script>

<div class="flex flex-wrap items-center gap-2 border-y border-line py-2">
  <div class="flex border border-line">
    <button
      class="px-3 py-1 text-[10px] uppercase tracking-[0.14em] {app.view === 'table'
        ? 'bg-ink text-panel font-bold'
        : 'text-dim hover:text-text'}"
      onclick={() => (app.view = 'table')}
    >
      Table
    </button>
    <button
      class="border-l border-line px-3 py-1 text-[10px] uppercase tracking-[0.14em] {app.view ===
      'json'
        ? 'bg-ink text-panel font-bold'
        : 'text-dim hover:text-text'}"
      onclick={() => (app.view = 'json')}
    >
      JSON
    </button>
  </div>

  <div class="relative">
    <button class="btn" onclick={() => (menu = menu === 'add' ? null : 'add')}>Add Data ▾</button>
    {#if menu === 'add'}
      <div class="panel absolute left-0 z-40 mt-2 w-48 p-1">
        <button class="menu-item" onclick={() => { menu = null; openNewDoc(); }}>Insert Document</button>
        <button class="menu-item" onclick={() => { menu = null; openBulkDocs(); }}>Insert Documents</button>
        <button class="menu-item" onclick={() => { menu = null; fileInput.click(); }}>Import File</button>
      </div>
    {/if}
  </div>

  <div class="relative">
    <button class="btn" onclick={() => (menu = menu === 'export' ? null : 'export')}>Export ▾</button>
    {#if menu === 'export'}
      <div class="panel absolute left-0 z-40 mt-2 w-44 p-1">
        <button class="menu-item" onclick={() => { menu = null; chooseExport('json'); }}>Export JSON</button>
        <button class="menu-item" onclick={() => { menu = null; chooseExport('jsonl'); }}>Export JSONL</button>
        <button class="menu-item" onclick={() => { menu = null; chooseExport('csv'); }}>Export CSV</button>
      </div>
    {/if}
  </div>

  <button class="btn btn-ghost" onclick={runFind} disabled={app.busy}>Refresh</button>
  <input bind:this={fileInput} type="file" accept=".json,.jsonl,.ndjson" class="hidden" onchange={pickFile} />

  <div class="ml-auto flex items-center gap-3 text-[11px] text-dim">
    <span>
      Documents
      <span class="tabular text-text">{start}–{end}</span>
      of <span class="tabular text-cyan">{app.result.total}</span>
    </span>
    <div class="flex gap-1">
      <button class="btn btn-ghost" disabled={!hasPrev || app.busy} onclick={() => step(-1)}>‹</button>
      <button class="btn btn-ghost" disabled={!hasNext || app.busy} onclick={() => step(1)}>›</button>
    </div>
  </div>
</div>
