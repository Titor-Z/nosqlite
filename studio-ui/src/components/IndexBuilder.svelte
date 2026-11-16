<script>
  import { createIndexFromBuilder } from '../lib/store.svelte.js';

  let { onClose } = $props();

  let name = $state('');
  let rows = $state([{ field: '', dir: 1 }]);
  let unique = $state(false);
  let sparse = $state(false);

  function addRow() {
    rows = [...rows, { field: '', dir: 1 }];
  }

  function removeRow(index) {
    rows = rows.filter((_, i) => i !== index);
  }

  function toggleDir(index) {
    rows = rows.map((row, i) => (i === index ? { ...row, dir: row.dir === 1 ? -1 : 1 } : row));
  }

  function submit() {
    createIndexFromBuilder({ name, rows, unique, sparse }).then((ok) => {
      if (ok === true) onClose();
    });
  }
</script>

<div class="fixed inset-0 z-[70] flex items-center justify-center p-4">
  <button
    class="absolute inset-0 cursor-default bg-void/85 backdrop-blur-[2px]"
    aria-label="关闭"
    onclick={onClose}
  ></button>
  <div class="panel relative z-10 w-[min(620px,94vw)] p-4">
    <header class="mb-4 flex items-center gap-2">
      <span class="text-cyan">◈</span>
      <span class="text-[11px] tracking-[0.2em] text-text">CREATE INDEX</span>
      <button class="ml-auto text-dim hover:text-cyan" onclick={onClose}>✕</button>
    </header>

    <div class="space-y-3">
      <label class="block">
        <span class="label mb-1 block">Name(可空,自动生成)</span>
        <input class="field" bind:value={name} placeholder="age_1" spellcheck="false" />
      </label>

      <div>
        <span class="label mb-1 block">Fields</span>
        <div class="space-y-2">
          {#each rows as row, i}
            <div class="flex items-center gap-2">
              <input
                class="field flex-1"
                bind:value={row.field}
                placeholder="字段路径,如 tenant 或 a.b"
                spellcheck="false"
              />
              <button class="btn w-14" type="button" onclick={() => toggleDir(i)}>
                {row.dir === 1 ? '↑ 1' : '↓ -1'}
              </button>
              <button
                class="btn btn-danger"
                type="button"
                onclick={() => removeRow(i)}
                disabled={rows.length === 1}
              >
                ✕
              </button>
            </div>
          {/each}
        </div>
        <button class="btn btn-ghost mt-2" type="button" onclick={addRow}>+ 添加字段</button>
      </div>

      <div class="flex gap-4 text-[11px]">
        <label class="flex items-center gap-1.5">
          <input type="checkbox" bind:checked={unique} class="accent-cyan" /> unique
        </label>
        <label class="flex items-center gap-1.5">
          <input type="checkbox" bind:checked={sparse} class="accent-cyan" /> sparse
        </label>
      </div>
    </div>

    <footer class="mt-5 flex justify-end gap-2">
      <button class="btn" onclick={onClose}>取消</button>
      <button class="btn btn-primary" onclick={submit} disabled={!rows.some((r) => r.field.trim())}>
        创建索引
      </button>
    </footer>
  </div>
</div>
