<script>
  import {
    app,
    runFind,
    resetQuery,
    runExplain,
    applyHistory,
  } from '../lib/store.svelte.js';

  // 根菜单 → options / history 面板;单一 ⋮ 入口收敛所有次要动作
  let menu = $state(null);
  const EMPTY_OBJECT = '{}';

  function submit(event) {
    event.preventDefault();
    runFind();
  }

  function pickHistory(entry) {
    menu = null;
    applyHistory(entry);
  }

  function act(fn) {
    menu = null;
    fn();
  }
</script>

<form class="space-y-2" onsubmit={submit}>
  <div class="flex items-end gap-2">
    <div class="grid min-w-0 flex-1 gap-2 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)]">
      <label class="block min-w-0">
        <span class="label mb-1 block">Filter</span>
        <input
          class="field"
          aria-label="Filter"
          bind:value={app.query.filter}
          spellcheck="false"
          placeholder={EMPTY_OBJECT}
        />
      </label>
      <label class="block min-w-0">
        <span class="label mb-1 block">Project</span>
        <input
          class="field"
          aria-label="Project"
          bind:value={app.query.project}
          spellcheck="false"
          placeholder={EMPTY_OBJECT}
        />
      </label>
      <label class="block min-w-0">
        <span class="label mb-1 block">Sort</span>
        <input
          class="field"
          aria-label="Sort"
          bind:value={app.query.sort}
          spellcheck="false"
          placeholder={EMPTY_OBJECT}
        />
      </label>
    </div>

    <button class="btn btn-primary" type="submit" disabled={app.busy}>Find</button>

    <div class="relative">
      <button
        class="btn btn-ghost px-2.5"
        type="button"
        aria-label="更多查询操作"
        title="更多"
        onclick={() => (menu = menu ? null : 'root')}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
          <circle cx="8" cy="3" r="1.4" />
          <circle cx="8" cy="8" r="1.4" />
          <circle cx="8" cy="13" r="1.4" />
        </svg>
      </button>

      {#if menu}
        <div class="panel shadow-hard absolute right-0 z-50 mt-2 w-64 p-1">
          {#if menu === 'root'}
            <button class="menu-item" type="button" onclick={() => act(resetQuery)}>Reset</button>
            <button class="menu-item" type="button" onclick={() => act(runExplain)}>Explain</button>
            <button class="menu-item" type="button" onclick={() => (menu = 'options')}>
              Options ▸
            </button>
            <button
              class="menu-item flex items-center justify-between gap-2"
              type="button"
              disabled={app.history.length === 0}
              onclick={() => (menu = 'history')}
            >
              <span>History</span>
              <span class="tag tabular">{app.history.length}</span>
            </button>
          {:else if menu === 'options'}
            <button class="menu-item text-[10px] text-dim" type="button" onclick={() => (menu = 'root')}>
              ◂ Options
            </button>
            <div class="grid grid-cols-2 gap-2 border-t border-line p-2">
              <label class="block">
                <span class="label mb-1 block">Skip</span>
                <input
                  class="field"
                  aria-label="Skip"
                  bind:value={app.query.skip}
                  inputmode="numeric"
                  placeholder="0"
                />
              </label>
              <label class="block">
                <span class="label mb-1 block">Limit</span>
                <input
                  class="field"
                  aria-label="Limit"
                  bind:value={app.query.limit}
                  inputmode="numeric"
                  placeholder="20"
                />
              </label>
              <label class="col-span-2 block opacity-45">
                <span class="label mb-1 block">Collation · NOT SUPPORTED</span>
                <input class="field" disabled placeholder="—" />
              </label>
              <label class="col-span-2 block opacity-45">
                <span class="label mb-1 block">Max Time · NOT SUPPORTED</span>
                <input class="field" disabled placeholder="—" />
              </label>
            </div>
          {:else}
            <button class="menu-item text-[10px] text-dim" type="button" onclick={() => (menu = 'root')}>
              ◂ History
            </button>
            <div class="max-h-72 overflow-auto border-t border-line">
              {#each app.history as entry, i}
                <button
                  class="block w-full border-b border-line px-2 py-2 text-left text-[11px] last:border-0 hover:bg-panel-2"
                  type="button"
                  onclick={() => pickHistory(entry)}
                >
                  <div class="truncate">
                    <span class="text-dim">#{i + 1}</span>
                    {entry.filter || '{}'}
                  </div>
                  <div class="truncate text-[10px] text-dim">
                    sort {entry.sort || '—'} · skip {entry.skip || 0} · limit {entry.limit || '—'}
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</form>
