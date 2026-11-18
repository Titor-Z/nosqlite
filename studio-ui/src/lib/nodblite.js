/**
 * nodblite C ABI 的浏览器端封装。
 *
 * 与 `web/nodblite.js`(Node 版)同源,但这里只保留浏览器需要的部分:
 * 不加载 WASI、不做文件 I/O,只负责「线性内存传参 + 解码 NodbliteResult」。
 */

/** wasm32 布局:NodbliteSlice = { const char* ptr; size_t len; } → 8 字节。 */
const SLICE_SIZE = 8;

/** wasm32 布局:NodbliteResult 字段偏移(见 include/nodblite.h,repr(C))。 */
const RESULT = {
  code: 0,
  count: 8,
  affected: 16,
  docs: 24,
  valuePtr: 28,
  valueLen: 32,
};

export function allocString(exports, text) {
  const bytes = new TextEncoder().encode(text + '\0');
  const ptr = exports.nodblite_malloc(bytes.length);
  if (!ptr) throw new Error(`nodblite_malloc(${bytes.length}) 失败`);
  // 每次调用后重建视图:memory.grow 会让旧视图失效
  new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
  return { ptr, len: bytes.length };
}

export function freeString(exports, { ptr, len }) {
  if (ptr) exports.nodblite_free(ptr, len);
}

function readString(exports, ptr, len) {
  if (!ptr || len === 0) return null;
  return new TextDecoder().decode(new Uint8Array(exports.memory.buffer, ptr, len));
}

/** 解码并释放一个 NodbliteResult*(arena:只 free 一次)。 */
export function readResult(exports, resultPtr) {
  if (!resultPtr) throw new Error('结果为 null 指针');
  const view = new DataView(exports.memory.buffer);
  const code = view.getInt32(resultPtr + RESULT.code, true);
  const count = Number(view.getBigUint64(resultPtr + RESULT.count, true));
  const affected = Number(view.getBigUint64(resultPtr + RESULT.affected, true));
  const docsPtr = view.getUint32(resultPtr + RESULT.docs, true);
  const valuePtr = view.getUint32(resultPtr + RESULT.valuePtr, true);
  const valueLen = view.getUint32(resultPtr + RESULT.valueLen, true);

  const docs = [];
  for (let i = 0; i < count; i++) {
    const base = docsPtr + i * SLICE_SIZE;
    const p = view.getUint32(base, true);
    const n = view.getUint32(base + 4, true);
    docs.push(readString(exports, p, n));
  }
  const value = readString(exports, valuePtr, valueLen);

  exports.nodblite_result_free(resultPtr);
  return { code, count, affected, docs, value };
}

/** 读线程局部错误信息(wasm32 布局)。 */
export function lastError(exports) {
  const code = exports.nodblite_last_error_code();
  if (code === 0) return null;
  const errPtr = exports.nodblite_last_error();
  if (!errPtr) return { code, codeStr: null, message: null, op: null };
  const view = new DataView(exports.memory.buffer);
  const slice = (offset) => {
    const p = view.getUint32(errPtr + offset, true);
    const n = view.getUint32(errPtr + offset + 4, true);
    return readString(exports, p, n);
  };
  return {
    code,
    codeStr: slice(4),
    message: slice(12),
    op: slice(20),
  };
}

export function openDb(exports, path, createIfMissing = true) {
  const p = allocString(exports, path);
  let handle;
  try {
    handle = exports.nodblite_open(p.ptr, createIfMissing ? 1 : 0);
  } finally {
    freeString(exports, p);
  }
  if (!handle) {
    const err = lastError(exports);
    throw new Error(`打开数据库失败: ${err?.message ?? '未知错误'}`);
  }
  return new NodbliteDb(exports, handle);
}

export class NodbliteDb {
  constructor(exports, handle) {
    this.exports = exports;
    this.handle = handle;
  }

  /** 调用 `(db, coll, ...strings, ...nums)` 形态的导出,自动管理字符串生命周期。 */
  #call(fn, coll, strings = [], nums = []) {
    const strs = [coll, ...strings].map((s) => (s === null || s === undefined ? null : s));
    const allocs = strs.filter((s) => s !== null).map((s) => allocString(this.exports, s));
    let allocIdx = 0;
    try {
      const args = [
        this.handle,
        ...strs.map((s) => (s === null ? 0 : allocs[allocIdx++].ptr)),
        ...nums,
      ];
      const result = readResult(this.exports, fn(...args));
      if (result.code !== 0) {
        const err = lastError(this.exports);
        const e = new Error(err?.message ?? `操作失败(code=${result.code})`);
        e.code = result.code;
        e.codeStr = err?.codeStr;
        throw e;
      }
      return result;
    } finally {
      for (const a of allocs) freeString(this.exports, a);
    }
  }

  insert(collection, doc) {
    return this.#call(this.exports.nodblite_insert, collection, [JSON.stringify(doc)]).value;
  }

  insertMany(collection, docs) {
    const r = this.#call(this.exports.nodblite_insert_many, collection, [JSON.stringify(docs)]);
    return JSON.parse(r.value ?? '[]');
  }

  get(collection, id) {
    const r = this.#call(this.exports.nodblite_get, collection, [id]);
    return r.value ? JSON.parse(r.value) : null;
  }

  find(collection, filter = {}, { sort = null, fields = null, skip = 0, limit = null } = {}) {
    const r = this.#call(
      this.exports.nodblite_find,
      collection,
      [
        JSON.stringify(filter),
        sort ? JSON.stringify(sort) : null,
        fields ? JSON.stringify(fields) : null,
      ],
      [BigInt(skip), BigInt(limit ?? 0), limit === null ? 1 : 0],
    );
    return r.docs.map((d) => JSON.parse(d));
  }

  /** 分页查询:一次扫描同时返回匹配总数与本页文档。 */
  findPage(collection, filter = {}, { sort = null, fields = null, skip = 0, limit = 25 } = {}) {
    const r = this.#call(
      this.exports.nodblite_find_page,
      collection,
      [
        JSON.stringify(filter),
        sort ? JSON.stringify(sort) : null,
        fields ? JSON.stringify(fields) : null,
      ],
      [BigInt(skip), BigInt(limit ?? 0), limit === null ? 1 : 0],
    );
    return { total: r.affected, docs: r.docs.map((d) => JSON.parse(d)) };
  }

  /**
   * 更新,返回实际修改条数。
   *
   * `opts.arrayFilters` 用于 `$[id]` 位置运算符(与 MongoDB 同名同语义)。
   */
  update(collection, filter, update, opts = {}) {
    if (opts.arrayFilters !== undefined) {
      return this.#call(this.exports.nodblite_update_array_filters, collection, [
        JSON.stringify(filter),
        JSON.stringify(update),
        JSON.stringify(opts.arrayFilters),
      ]).affected;
    }
    return this.#call(this.exports.nodblite_update, collection, [
      JSON.stringify(filter),
      JSON.stringify(update),
    ]).affected;
  }

  upsert(collection, filter, update) {
    const r = this.#call(this.exports.nodblite_upsert, collection, [
      JSON.stringify(filter),
      JSON.stringify(update),
    ]);
    return r.value ? JSON.parse(r.value) : null;
  }

  delete(collection, filter) {
    return this.#call(this.exports.nodblite_delete, collection, [JSON.stringify(filter)]).affected;
  }

  count(collection, filter = {}) {
    return this.#call(this.exports.nodblite_count, collection, [JSON.stringify(filter)]).affected;
  }

  listCollections() {
    const r = readResult(this.exports, this.exports.nodblite_list_collections(this.handle));
    if (r.code !== 0) throw new Error(lastError(this.exports)?.message ?? '列出集合失败');
    return r.docs.map((d) => JSON.parse(d));
  }

  dropCollection(collection) {
    return this.#call(this.exports.nodblite_drop_collection, collection).affected;
  }

  createIndex(collection, name, field, { unique = false, sparse = false } = {}) {
    this.#call(this.exports.nodblite_create_index, collection, [name, field], [
      unique ? 1 : 0,
      sparse ? 1 : 0,
    ]);
  }

  createCompositeIndex(collection, name, fields, directions, { unique = false, sparse = false } = {}) {
    this.#call(
      this.exports.nodblite_create_index_composite,
      collection,
      [name, JSON.stringify(fields), JSON.stringify(directions)],
      [unique ? 1 : 0, sparse ? 1 : 0],
    );
  }

  /** 列出集合的索引定义。C ABI 把 JSON 数组放在 `value` 里。 */
  listIndexes(collection) {
    const r = this.#call(this.exports.nodblite_list_indexes, collection);
    return r.value ? JSON.parse(r.value) : [];
  }

  dropIndex(collection, name) {
    this.#call(this.exports.nodblite_drop_index, collection, [name]);
  }

  /** 执行聚合管道。返回输出文档数组。 */
  aggregate(collection, pipeline) {
    const r = this.#call(this.exports.nodblite_aggregate, collection, [
      JSON.stringify(pipeline),
    ]);
    return r.docs.map((d) => JSON.parse(d));
  }

  explain(collection, filter, sort = null) {
    const r = this.#call(this.exports.nodblite_explain, collection, [
      JSON.stringify(filter),
      sort ? JSON.stringify(sort) : null,
    ]);
    return r.value ? JSON.parse(r.value) : null;
  }

  /** 压缩数据库文件(把空闲页还给文件系统)。studio 内为内存文件系统,主要是保持一致。 */
  compact() {
    this.#call(this.exports.nodblite_compact, null);
    return true;
  }

  close() {
    if (this.handle) {
      this.exports.nodblite_close(this.handle);
      this.handle = null;
    }
  }
}
