/**
 * 全局状态与所有数据库动作。
 *
 * 信息架构对标 MongoDB Compass Data Explorer:
 * 左侧集合树 + 单集合工作区,二级标签 Documents / Indexes / Explain / Schema / Aggregation。
 *
 * Svelte 5 runes:`.svelte.js` 里可用 `$state`。
 */

import { StudioDb } from './db.js';

const studio = new StudioDb();

const DEFAULT_QUERY = () => ({
  filter: '{}',
  project: '',
  sort: '',
  skip: '',
  limit: '20',
});

export const app = $state({
  ready: false,
  busy: false,
  opened: false,
  fileName: '',
  fileSize: 0,

  collections: [],
  /** 集合文档数(估算,延迟填充):name → count */
  docCounts: {},
  selected: null,

  tab: 'documents',
  view: 'table',

  query: DEFAULT_QUERY(),
  result: { docs: [], total: 0 },
  queryPlan: null,

  history: [],
  indexes: [],

  explain: null,
  schema: { sampleSize: 200, fields: [], analyzed: false, sampleCount: 0 },

  pipeline: [],
  aggregation: { docs: [], partials: [], error: '' },

  editor: null,
  confirm: null,
  toast: null,
  logs: [],
});

let pendingAction = null;
let toastTimer = null;

function stamp() {
  return new Date().toLocaleTimeString('zh-CN', { hour12: false });
}

export function log(message) {
  app.logs = [`${stamp()}  ${message}`, ...app.logs].slice(0, 60);
}

export function notify(kind, text) {
  app.toast = { kind, text };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    app.toast = null;
  }, 3600);
}

function guard(label, fn) {
  return (async () => {
    app.busy = true;
    try {
      return await fn();
    } catch (e) {
      const msg = e?.message ?? String(e);
      console.error(`[${label}]`, e);
      notify('error', msg);
      log(`${label} ✗ ${msg}`);
      return undefined;
    } finally {
      app.busy = false;
    }
  })();
}

function parseJson(text, label) {
  try {
    return JSON.parse(text);
  } catch (e) {
    throw new Error(`${label} 不是合法 JSON: ${e.message}`);
  }
}

export function bytesLabel(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

/* ---------------- 启动与数据库生命周期 ---------------- */

export function boot() {
  return guard('引擎启动', async () => {
    await studio.init();
    app.ready = true;
    log('WASM 引擎已就绪');
  });
}

function afterOpen(name) {
  app.fileName = name;
  app.fileSize = studio.size();
  app.opened = true;
  app.tab = 'documents';
  app.view = 'table';
  app.query = DEFAULT_QUERY();
  app.result = { docs: [], total: 0 };
  app.queryPlan = null;
  app.explain = null;
  app.schema = { sampleSize: 200, fields: [], analyzed: false, sampleCount: 0 };
  app.pipeline = [];
  app.aggregation = { docs: [], partials: [], error: '' };
  app.docCounts = {};
  log(`打开数据库 ${name}`);
}

export function importFile(file) {
  if (!file) return;
  if (!file.name.toLowerCase().endsWith('.nodb')) {
    notify('error', `只支持 .nodb 文件:${file.name}`);
    return;
  }
  return guard('导入数据库', async () => {
    const bytes = new Uint8Array(await file.arrayBuffer());
    app.opened = false;
    studio.open(bytes);
    afterOpen(file.name);
    await reloadCollections();
    notify('ok', `已载入 ${file.name}`);
  });
}

export function newDatabase() {
  return guard('新建数据库', async () => {
    app.opened = false;
    studio.open(null);
    afterOpen('untitled.nodb');
    await reloadCollections();
    notify('ok', '已创建空数据库');
  });
}

export function exportDatabase() {
  if (!app.opened) return;
  try {
    const bytes = studio.bytes();
    download(
      new Blob([bytes], { type: 'application/octet-stream' }),
      app.fileName || 'database.nodb',
    );
    log(`导出数据库 ${app.fileName} (${bytesLabel(bytes.byteLength)})`);
    notify('ok', '数据库已导出');
  } catch (e) {
    notify('error', e.message ?? String(e));
  }
}

function download(blob, filename) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/* ---------------- 集合与查询 ---------------- */

async function reloadCollections() {
  if (!app.opened) return;
  app.collections = studio.db.listCollections();
  if (!app.selected || !app.collections.includes(app.selected)) {
    app.selected = app.collections[0] ?? null;
  }
  await loadCollection();
  refreshCounts();
}

export function refresh() {
  return guard('刷新', async () => {
    await reloadCollections();
    log('已刷新');
  });
}

/** 加载当前集合:查询结果 + 索引 + 计数。 */
async function loadCollection() {
  if (!app.selected) {
    app.result = { docs: [], total: 0 };
    app.indexes = [];
    return;
  }
  app.query = DEFAULT_QUERY();
  runFind();
  loadIndexes();
}

export function selectCollection(name) {
  if (app.selected === name) return;
  app.selected = name;
  app.tab = 'documents';
  app.queryPlan = null;
  app.explain = null;
  app.schema = { sampleSize: 200, fields: [], analyzed: false, sampleCount: 0 };
  app.pipeline = [];
  app.aggregation = { docs: [], partials: [], error: '' };
  return guard('加载集合', async () => {
    await loadCollection();
  });
}

/** 延迟统计每个集合的文档数(侧栏用)。 */
function refreshCounts() {
  if (!app.opened) return;
  for (const name of app.collections) {
    try {
      app.docCounts = { ...app.docCounts, [name]: studio.db.count(name) };
    } catch {
      /* 集合刚被删除时忽略 */
    }
  }
}

function currentQuery() {
  const filter = parseJson(app.query.filter || '{}', 'Filter');
  const sort = app.query.sort.trim() ? parseJson(app.query.sort, 'Sort') : null;
  const project = app.query.project.trim() ? parseJson(app.query.project, 'Project') : null;
  const skip = app.query.skip.trim() ? Number(app.query.skip) : 0;
  const limit = app.query.limit.trim() ? Number(app.query.limit) : 20;
  if (!Number.isInteger(skip) || skip < 0) throw new Error('Skip 必须是非负整数');
  if (!Number.isInteger(limit) || limit < 0) throw new Error('Limit 必须是非负整数');
  return { filter, sort, project, skip, limit };
}

export function runFind() {
  if (!app.selected) return;
  return guard('查询', async () => {
    const { filter, sort, project, skip, limit } = currentQuery();
    const { total, docs } = studio.db.findPage(app.selected, filter, {
      sort,
      fields: project,
      skip,
      limit,
    });
    app.result = { docs, total };
    app.queryPlan = studio.db.explain(app.selected, filter, sort);
    log(`查询 ${app.selected}:命中 ${total} 条 (${app.queryPlan?.stage ?? '?'})`);

    const entry = {
      filter: app.query.filter,
      project: app.query.project,
      sort: app.query.sort,
      skip: app.query.skip,
      limit: app.query.limit,
    };
    const key = JSON.stringify(entry);
    if (app.history[0]?.key !== key) {
      app.history = [{ key, ...entry }, ...app.history].slice(0, 20);
    }
  });
}

export function resetQuery() {
  app.query = DEFAULT_QUERY();
  return runFind();
}

export function applyHistory(entry) {
  app.query = {
    filter: entry.filter,
    project: entry.project,
    sort: entry.sort,
    skip: entry.skip,
    limit: entry.limit,
  };
  return runFind();
}

export function gotoRange(delta) {
  const { limit, skip } = currentQuery();
  const next = Math.max(0, skip + delta * limit);
  app.query.skip = next ? String(next) : '';
  return runFind();
}

/** 表头点击排序:升序 → 降序 → 取消。 */
export function toggleSort(field) {
  let current = {};
  try {
    current = app.query.sort.trim() ? JSON.parse(app.query.sort) : {};
  } catch {
    current = {};
  }
  const currentDir = current?.[field];
  const next = currentDir === 1 ? -1 : currentDir === -1 ? undefined : 1;
  const sort = {};
  if (next) sort[field] = next;
  app.query.sort = next ? JSON.stringify(sort) : '';
  app.query.skip = '';
  return runFind();
}

/* ---------------- 文档编辑 ---------------- */

export function openNewDoc() {
  app.editor = {
    mode: 'new',
    collection: app.selected ?? '',
    text: '{\n  \n}',
    error: '',
  };
}

export function openBulkDocs() {
  app.editor = {
    mode: 'bulk',
    collection: app.selected ?? '',
    text: '[\n  \n]',
    error: '',
  };
}

export function openEditDoc(doc) {
  app.editor = {
    mode: 'edit',
    id: doc._id,
    collection: app.selected,
    text: JSON.stringify(doc, null, 2),
    original: doc,
    error: '',
  };
}

export function duplicateDoc(doc) {
  const copy = { ...doc };
  delete copy._id;
  return guard('复制文档', async () => {
    const id = studio.db.insert(app.selected, copy);
    log(`复制 ${app.selected}/${doc._id} → ${id}`);
    notify('ok', '文档已复制');
    await loadCollection();
    refreshCounts();
  });
}

export function closeEditor() {
  app.editor = null;
}

/** 保存编辑器:单条插入 / 批量插入 / 编辑($set + $unset 增量)。 */
export function saveEditor() {
  const editor = app.editor;
  if (!editor) return;
  return guard('保存文档', async () => {
    const collection = (editor.collection || app.selected || '').trim();
    if (!collection) throw new Error('请填写集合名');

    if (editor.mode === 'bulk') {
      const docs = parseJson(editor.text, '文档数组');
      if (!Array.isArray(docs)) throw new Error('批量插入需要 JSON 数组');
      const ids = studio.db.insertMany(collection, docs);
      log(`批量插入 ${collection}:${ids.length} 条`);
      notify('ok', `已插入 ${ids.length} 条文档`);
    } else if (editor.mode === 'new') {
      const next = parseJson(editor.text, '文档');
      if (!next || typeof next !== 'object' || Array.isArray(next)) {
        throw new Error('文档必须是 JSON 对象');
      }
      const id = studio.db.insert(collection, next);
      log(`插入 ${collection}/${id}`);
      notify('ok', '文档已插入');
    } else {
      const next = parseJson(editor.text, '文档');
      if (!next || typeof next !== 'object' || Array.isArray(next)) {
        throw new Error('文档必须是 JSON 对象');
      }
      const original = editor.original ?? {};
      const set = {};
      const unset = {};
      for (const [k, v] of Object.entries(next)) {
        if (k === '_id') continue;
        if (JSON.stringify(v) !== JSON.stringify(original[k])) set[k] = v;
      }
      for (const k of Object.keys(original)) {
        if (k !== '_id' && !(k in next)) unset[k] = '';
      }
      const update = {};
      if (Object.keys(set).length) update.$set = set;
      if (Object.keys(unset).length) update.$unset = unset;
      if (!Object.keys(update).length) {
        notify('ok', '内容未变化');
        app.editor = null;
        return;
      }
      studio.db.update(app.selected, { _id: editor.id }, update);
      log(`更新 ${app.selected}/${editor.id}`);
      notify('ok', '文档已保存');
    }

    app.selected = collection;
    app.editor = null;
    app.collections = studio.db.listCollections();
    await loadCollection();
    refreshCounts();
  });
}

export function askDeleteDoc(doc) {
  askConfirm('删除文档', `确定删除 ${app.selected}/${doc._id} ?`, () => {
    guard('删除文档', async () => {
      studio.db.delete(app.selected, { _id: doc._id });
      log(`删除 ${app.selected}/${doc._id}`);
      notify('ok', '文档已删除');
      await loadCollection();
      refreshCounts();
    });
  });
}

export function askDropCollection(name) {
  askConfirm(
    '删除集合',
    `确定删除集合「${name}」及其全部文档与索引?`,
    () => {
      guard('删除集合', async () => {
        const n = studio.db.dropCollection(name);
        log(`删除集合 ${name} (${n} 条文档)`);
        notify('ok', `集合 ${name} 已删除`);
        if (app.selected === name) app.selected = null;
        await reloadCollections();
      });
    },
    true,
  );
}

/* ---------------- 集合级导入导出 ---------------- */

/** 集合级导入:JSON 数组或 JSON Lines。 */
export function importCollectionFile(file) {
  if (!file) return;
  return guard('导入集合', async () => {
    const text = await file.text();
    let docs;
    try {
      const parsed = JSON.parse(text);
      docs = Array.isArray(parsed) ? parsed : [parsed];
    } catch {
      docs = text
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line, i) => {
          try {
            return JSON.parse(line);
          } catch (e) {
            throw new Error(`第 ${i + 1} 行不是合法 JSON: ${e.message}`);
          }
        });
    }
    if (!Array.isArray(docs) || docs.length === 0) throw new Error('没有可导入的文档');
    const ids = studio.db.insertMany(app.selected, docs);
    log(`导入 ${app.selected}:${ids.length} 条`);
    notify('ok', `已导入 ${ids.length} 条文档`);
    await loadCollection();
    refreshCounts();
  });
}

/** 集合级导出:json / jsonl / csv。 */
export function exportCollection(format) {
  if (!app.selected) return;
  return guard('导出集合', async () => {
    const { filter, sort, project } = currentQuery();
    const pageSize = 1000;
    const docs = [];
    let skip = 0;
    for (;;) {
      const { docs: batch, total } = studio.db.findPage(app.selected, filter, {
        sort,
        fields: project,
        skip,
        limit: pageSize,
      });
      docs.push(...batch);
      skip += batch.length;
      if (batch.length === 0 || skip >= total) break;
    }

    let text;
    let mime = 'application/json';
    if (format === 'jsonl') {
      text = docs.map((d) => JSON.stringify(d)).join('\n') + '\n';
      mime = 'application/x-ndjson';
    } else if (format === 'csv') {
      const columns = [];
      const seen = new Set();
      for (const d of docs) {
        for (const k of Object.keys(d)) {
          if (!seen.has(k)) {
            seen.add(k);
            columns.push(k);
          }
        }
      }
      const esc = (v) => `"${String(v).replaceAll('"', '""')}"`;
      const lines = [columns.map(esc).join(',')];
      for (const d of docs) {
        lines.push(
          columns
            .map((c) => {
              const v = d[c];
              if (v === undefined) return '""';
              return esc(typeof v === 'object' && v !== null ? JSON.stringify(v) : v);
            })
            .join(','),
        );
      }
      text = lines.join('\n') + '\n';
      mime = 'text/csv';
    } else {
      text = JSON.stringify(docs, null, 2);
    }

    const ext = format === 'jsonl' ? 'jsonl' : format;
    download(new Blob([text], { type: mime }), `${app.selected}.${ext}`);
    log(`导出 ${app.selected}.${ext}:${docs.length} 条`);
    notify('ok', `已导出 ${docs.length} 条文档`);
  });
}

/* ---------------- 索引 ---------------- */

function loadIndexes() {
  if (!app.selected) {
    app.indexes = [];
    return;
  }
  app.indexes = studio.db.listIndexes(app.selected);
}

export function createIndex(form) {
  return guard('创建索引', async () => {
    const fields = form.fields
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    if (!fields.length) throw new Error('至少需要一个字段');
    const directions = form.directions
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((s) => (s === '-1' ? -1 : 1));
    if (directions.length && directions.length !== fields.length) {
      throw new Error('方向数量必须与字段数量一致');
    }
    const name = form.name.trim() || fields.map((f, i) => `${f}_${directions[i] ?? 1}`).join('_');
    if (fields.length === 1) {
      studio.db.createIndex(app.selected, name, fields[0], form);
    } else {
      studio.db.createCompositeIndex(app.selected, name, fields, directions, form);
    }
    log(`创建索引 ${app.selected}/${name} (${fields.join(', ')})`);
    notify('ok', `索引 ${name} 已创建`);
    loadIndexes();
    return true;
  });
}

export function askDropIndex(name) {
  askConfirm(
    '删除索引',
    `确定删除索引「${name}」?`,
    () => {
      guard('删除索引', async () => {
        studio.db.dropIndex(app.selected, name);
        log(`删除索引 ${app.selected}/${name}`);
        notify('ok', `索引 ${name} 已删除`);
        loadIndexes();
      });
    },
    true,
  );
}

/** 由 builder 提交的字段数组创建索引(方向由 fields[i].dir 决定)。 */
export function createIndexFromBuilder({ name, rows, unique, sparse }) {
  const fields = rows.map((r) => r.field.trim()).filter(Boolean);
  const directions = rows.filter((r) => r.field.trim()).map((r) => (r.dir === -1 ? -1 : 1));
  if (!fields.length) {
    notify('error', '至少需要一个字段');
    return;
  }
  return createIndex({
    name,
    fields: fields.join(','),
    directions: directions.join(','),
    unique,
    sparse,
  });
}

/* ---------------- Explain ---------------- */

export function runExplain() {
  if (!app.selected) return;
  return guard('执行计划', async () => {
    const { filter, sort, skip, limit } = currentQuery();
    const t0 = performance.now();
    const { total, docs } = studio.db.findPage(app.selected, filter, {
      sort,
      skip,
      limit,
    });
    const elapsed = performance.now() - t0;
    const plan = studio.db.explain(app.selected, filter, sort);
    app.explain = {
      plan,
      metrics: { returned: docs.length, total, elapsed, limit, skip },
      executedAt: stamp(),
    };
    log(`执行计划:${plan?.stage ?? '?'} (${elapsed.toFixed(2)} ms)`);
  });
}

/* ---------------- Schema ---------------- */

function typeOf(value) {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

function collectSchema(doc, prefix, map, total) {
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) return;
  for (const [key, value] of Object.entries(doc)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (!map.has(path)) map.set(path, { path, seen: 0, types: {} });
    const entry = map.get(path);
    entry.seen += 1;
    const type = typeOf(value);
    entry.types[type] = (entry.types[type] ?? 0) + 1;
    if (type === 'object' && value !== null) collectSchema(value, path, map, total);
  }
}

export function runSchema(sampleSize) {
  if (!app.selected) return;
  app.schema.sampleSize = sampleSize;
  return guard('Schema 采样', async () => {
    const { docs } = studio.db.findPage(app.selected, {}, { limit: sampleSize });
    const map = new Map();
    for (const doc of docs) collectSchema(doc, '', map, docs.length);
    app.schema = {
      sampleSize,
      sampleCount: docs.length,
      analyzed: true,
      fields: [...map.values()].sort((a, b) => a.path.localeCompare(b.path)),
    };
    log(`Schema 采样 ${app.selected}:${docs.length} 条 / ${map.size} 个字段`);
  });
}

/* ---------------- Aggregation ---------------- */

/** 追加一个阶段。`template` 缺省为 `$match`,由 AggregationPane 按所选阶段传入。 */
export function addStage(template = '{\n  "$match": {}\n}') {
  app.pipeline = [...app.pipeline, { text: template }];
}

export function removeStage(index) {
  app.pipeline = app.pipeline.filter((_, i) => i !== index);
}

export function moveStage(index, delta) {
  const next = index + delta;
  if (next < 0 || next >= app.pipeline.length) return;
  const copy = [...app.pipeline];
  [copy[index], copy[next]] = [copy[next], copy[index]];
  app.pipeline = copy;
}

export function runAggregation() {
  if (!app.selected) return;
  return guard('聚合', async () => {
    const pipeline = app.pipeline.map((s, i) => {
      try {
        return JSON.parse(s.text);
      } catch (e) {
        throw new Error(`Stage ${i + 1} 不是合法 JSON: ${e.message}`);
      }
    });
    const partials = [];
    for (let i = 0; i < pipeline.length; i++) {
      try {
        const docs = studio.db.aggregate(app.selected, pipeline.slice(0, i + 1));
        partials.push({ count: docs.length, docs: docs.slice(0, 5), error: '' });
      } catch (e) {
        partials.push({ count: 0, docs: [], error: e.message });
        throw new Error(`Stage ${i + 1}: ${e.message}`);
      }
    }
    const docs = studio.db.aggregate(app.selected, pipeline);
    app.aggregation = { docs, partials, error: '' };
    log(`聚合 ${app.selected}:${pipeline.length} 个 stage,输出 ${docs.length} 条`);
  });
}

/* ---------------- 弹窗 ---------------- */

export function askConfirm(title, text, action, danger = false) {
  pendingAction = action;
  app.confirm = { title, text, danger };
}

export function confirmYes() {
  const action = pendingAction;
  pendingAction = null;
  app.confirm = null;
  action?.();
}

export function confirmNo() {
  pendingAction = null;
  app.confirm = null;
}

export function closeToast() {
  app.toast = null;
}
