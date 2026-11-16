<script>
  import { untrack } from 'svelte';
  import { app, closeEditor, saveEditor } from '../lib/store.svelte.js';
  import JsonEditor from './JsonEditor.svelte';

  let localError = $state('');
  let fieldError = $state('');
  let view = $state('json');
  let fields = $state([]);
  let lastEditor = null;

  const title = $derived(
    app.editor?.mode === 'new'
      ? 'INSERT DOCUMENT'
      : app.editor?.mode === 'bulk'
        ? 'INSERT DOCUMENTS'
        : 'EDIT DOCUMENT',
  );

  // 编辑器实例变化时重建字段行;text 不参与依赖,避免 Field 编辑时自我覆盖
  $effect(() => {
    const ed = app.editor;
    if (!ed) {
      lastEditor = null;
      fields = [];
      view = 'json';
      return;
    }
    if (ed === lastEditor) return;
    lastEditor = ed;
    view = 'json';
    fields = untrack(() => parseFields(ed.text));
    fieldError = '';
  });

  $effect(() => {
    const text = app.editor?.text ?? '';
    const mode = app.editor?.mode;
    if (!text.trim()) {
      localError = '';
      return;
    }
    try {
      const parsed = JSON.parse(text);
      if (mode === 'bulk' && !Array.isArray(parsed)) {
        localError = '批量插入需要 JSON 数组';
      } else if (mode !== 'bulk' && (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed))) {
        localError = '文档必须是 JSON 对象';
      } else {
        localError = '';
      }
    } catch (e) {
      localError = locateJsonError(e, text);
    }
  });

  function locateJsonError(error, text) {
    const match = /position (\d+)/.exec(error.message);
    if (!match) return error.message;
    const position = Math.min(Number(match[1]), text.length);
    const before = text.slice(0, position);
    const line = before.split('\n').length;
    const column = position - before.lastIndexOf('\n');
    return `${error.message} (第 ${line} 行,第 ${column} 列)`;
  }

  /* ---------- 字段视图:按类型编辑,从根上避免写错值类型 ---------- */

  function typeOf(v) {
    if (v === null) return 'null';
    if (Array.isArray(v)) return 'array';
    if (typeof v === 'object') {
      return Object.keys(v).length === 1 && '$date' in v ? 'date' : 'object';
    }
    if (typeof v === 'number') return 'number';
    if (typeof v === 'boolean') return 'boolean';
    return 'string';
  }

  function rawOf(v, type) {
    if (type === 'null') return '';
    if (type === 'date') return String(v.$date);
    if (type === 'array' || type === 'object') return JSON.stringify(v, null, 2);
    return String(v);
  }

  function parseFields(text) {
    try {
      const parsed = JSON.parse(text);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return [];
      return Object.entries(parsed).map(([key, value]) => {
        const type = typeOf(value);
        return { key, type, raw: rawOf(value, type) };
      });
    } catch {
      return [];
    }
  }

  function defaultValue(type) {
    if (type === 'string') return '';
    if (type === 'number') return '0';
    if (type === 'boolean') return 'true';
    if (type === 'null') return '';
    if (type === 'array') return '[]';
    if (type === 'object') return '{}';
    if (type === 'date') return new Date().toISOString();
    return '';
  }

  function changeType(field) {
    field.raw = defaultValue(field.type);
    commitFields();
  }

  function addField() {
    fields = [...fields, { key: '', type: 'string', raw: '' }];
    fieldError = '';
  }

  function removeField(index) {
    if (fields[index]?.key === '_id') return;
    fields = fields.filter((_, i) => i !== index);
    commitFields();
  }

  function commitFields() {
    if (!app.editor || view !== 'fields' || app.editor.mode === 'bulk') return;
    const out = {};
    let error = '';
    for (const f of fields) {
      const key = f.key.trim();
      if (!key) {
        error = '字段名不能为空';
        continue;
      }
      if (key in out) {
        error = `字段名重复:${key}`;
        continue;
      }
      if (f.type === 'string') {
        out[key] = f.raw;
      } else if (f.type === 'number') {
        const n = Number(f.raw);
        if (f.raw.trim() === '' || Number.isNaN(n)) error = `${key} 需要合法数字`;
        else out[key] = n;
      } else if (f.type === 'boolean') {
        out[key] = f.raw === 'true';
      } else if (f.type === 'null') {
        out[key] = null;
      } else if (f.type === 'date') {
        out[key] = { $date: f.raw };
      } else {
        try {
          const value = JSON.parse(f.raw || defaultValue(f.type));
          const isArray = Array.isArray(value);
          if (f.type === 'array' && !isArray) throw new Error('not array');
          if (f.type === 'object' && (isArray || value === null || typeof value !== 'object')) {
            throw new Error('not object');
          }
          out[key] = value;
        } catch {
          error = `${key} 不是合法 ${f.type === 'array' ? 'JSON 数组' : 'JSON 对象'}`;
        }
      }
    }
    fieldError = error;
    if (!error) app.editor.text = JSON.stringify(out, null, 2);
  }

  function switchView(next) {
    if (next === view) return;
    if (next === 'fields') {
      // 切到字段视图时以当前 JSON 为准重建行(JSON 视图可能刚被外部修改)
      fields = parseFields(app.editor?.text ?? '');
      fieldError = '';
    } else if (next === 'json' && view === 'fields') {
      commitFields();
      if (fieldError) return;
    }
    view = next;
  }

  function formatJson() {
    if (!app.editor) return;
    try {
      app.editor.text = JSON.stringify(JSON.parse(app.editor.text), null, 2);
      if (view === 'fields') fields = parseFields(app.editor.text);
    } catch {
      /* 非法 JSON 时保留原文,错误已由实时校验显示 */
    }
  }

  function onKeydown(event) {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      save();
    }
  }

  function save() {
    if (view === 'fields') {
      commitFields();
      if (fieldError) return;
    }
    if (!localError) saveEditor();
  }
</script>

{#if app.editor}
  <div class="fixed inset-0 z-[70] flex items-center justify-center p-4">
    <button
      class="absolute inset-0 cursor-default bg-void/85 backdrop-blur-[2px]"
      aria-label="关闭"
      onclick={closeEditor}
    ></button>
    <div
      class="panel shadow-hard-lg relative z-10 flex max-h-[90vh] w-[min(860px,94vw)] flex-col"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <header class="flex items-center gap-3 border-b border-line px-4 py-3">
        <span class="text-cyan">◈</span>
        <span class="text-[11px] font-bold tracking-[0.2em] text-text">{title}</span>
        {#if app.editor.id}
          <code class="truncate text-[10px] text-dim">{app.editor.id}</code>
        {/if}

        {#if app.editor.mode !== 'bulk'}
          <div class="ml-auto flex border border-ink">
            <button
              class="px-2 py-0.5 text-[10px] font-bold uppercase tracking-[0.12em] {view === 'json'
                ? 'bg-ink text-void'
                : 'text-dim hover:text-text'}"
              type="button"
              onclick={() => switchView('json')}
            >
              JSON
            </button>
            <button
              class="border-l border-ink px-2 py-0.5 text-[10px] font-bold uppercase tracking-[0.12em] {view ===
              'fields'
                ? 'bg-ink text-void'
                : 'text-dim hover:text-text'}"
              type="button"
              onclick={() => switchView('fields')}
            >
              Fields
            </button>
          </div>
        {/if}
        <button class="text-dim hover:text-cyan {app.editor.mode === 'bulk' ? 'ml-auto' : ''}" onclick={closeEditor}
          >✕</button
        >
      </header>

      {#if app.editor.mode !== 'edit'}
        <div class="flex items-center gap-2 border-b border-line px-4 py-2">
          <span class="label shrink-0">Collection</span>
          <input
            class="field flex-1"
            bind:value={app.editor.collection}
            placeholder="users"
            spellcheck="false"
          />
        </div>
      {/if}

      <div class="min-h-0 flex-1 overflow-auto p-4">
        {#if view === 'fields'}
          <div class="space-y-1">
            <div class="grid grid-cols-[minmax(120px,1fr)_110px_minmax(0,2fr)_32px] gap-1 pb-1">
              <span class="label">字段名</span>
              <span class="label">类型</span>
              <span class="label">值</span>
              <span></span>
            </div>
            {#each fields as field, i (i)}
              <div class="grid grid-cols-[minmax(120px,1fr)_110px_minmax(0,2fr)_32px] items-start gap-1">
                <input
                  class="field"
                  bind:value={field.key}
                  oninput={commitFields}
                  spellcheck="false"
                  placeholder="字段名"
                  readonly={field.key === '_id' && app.editor.mode === 'edit'}
                  class:opacity-60={field.key === '_id' && app.editor.mode === 'edit'}
                />
                <select
                  class="field"
                  bind:value={field.type}
                  onchange={() => changeType(field)}
                  disabled={field.key === '_id'}
                >
                  <option value="string">string</option>
                  <option value="number">number</option>
                  <option value="boolean">boolean</option>
                  <option value="null">null</option>
                  <option value="date">date</option>
                  <option value="array">array</option>
                  <option value="object">object</option>
                </select>
                {#if field.type === 'boolean'}
                  <select class="field" bind:value={field.raw} onchange={commitFields}>
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </select>
                {:else if field.type === 'null'}
                  <div class="field opacity-45">null</div>
                {:else if field.type === 'array' || field.type === 'object'}
                  <textarea
                    class="field h-[76px]"
                    bind:value={field.raw}
                    oninput={commitFields}
                    spellcheck="false"
                  ></textarea>
                {:else}
                  <input
                    class="field"
                    bind:value={field.raw}
                    oninput={commitFields}
                    spellcheck="false"
                    placeholder={field.type === 'date' ? '2026-11-15T00:00:00Z' : ''}
                  />
                {/if}
                <button
                  class="btn btn-ghost h-[32px] px-0"
                  type="button"
                  title="删除字段"
                  disabled={field.key === '_id'}
                  onclick={() => removeField(i)}>✕</button
                >
              </div>
            {/each}

            <button class="btn mt-2" type="button" onclick={addField}>+ 字段</button>
            {#if fieldError}
              <div class="mt-2 border border-danger/40 px-3 py-2 text-[11px] text-danger">{fieldError}</div>
            {/if}
          </div>
        {:else}
          <JsonEditor bind:value={app.editor.text} height="52vh" {onKeydown} />
          {#if localError}
            <div class="mt-2 border border-danger/40 px-3 py-2 text-[11px] text-danger">{localError}</div>
          {/if}
        {/if}
      </div>

      <footer class="flex items-center gap-2 border-t border-line px-4 py-3">
        <button class="btn btn-ghost" onclick={formatJson}>格式化</button>
        <span class="text-[10px] text-dim">
          {app.editor.mode === 'edit'
            ? '仅顶层字段变化会写入($_set / $unset),_id 不可修改'
            : app.editor.mode === 'bulk'
              ? 'JSON 数组;单事务写入,任一条失败整批回滚'
              : '_id 缺省时自动生成'}
        </span>
        <span class="ml-auto text-[10px] text-dim">⌘/Ctrl + Enter 保存</span>
        <button class="btn" onclick={closeEditor}>取消</button>
        <button
          class="btn btn-primary"
          onclick={save}
          disabled={app.busy || !!localError || !!fieldError}>保存</button
        >
      </footer>
    </div>
  </div>
{/if}
