/**
 * nodblite 的 WASM 宿主封装(JS / Node,零依赖)。
 *
 * 原理:同一份 Rust 源码编译到 wasm32-wasip1,导出全部 nodblite_* C ABI。
 * 本文件只做三件事:
 *   1. 用 WASI 运行时把宿主目录 preopen 给 wasm;
 *   2. 通过 nodblite_malloc/nodblite_free 在线性内存里传字符串;
 *   3. 解码 NodbliteResult 结构体(wasm32 布局,见下方常量)。
 *
 * 注意:所有内存视图在每次调用后重建 —— wasm `memory.grow` 会使旧视图失效。
 */

/** wasm32 布局:NodbliteSlice = { const char* ptr; size_t len; } → 8 字节。 */
const SLICE_SIZE = 8;
/** wasm32 布局:NodbliteResult 字段偏移(见 include/nodblite.h,repr(C))。
 * code@0,pad@4,count@8,affected@16,docs@24(value.ptr@28,value.len@32)。 */
const RESULT = {
  code: 0,
  count: 8,
  affected: 16,
  docs: 24,
  valuePtr: 28,
  valueLen: 32,
};

/**
 * 加载 wasm 并注册给 WASI。
 *
 * @param {object} opts
 * @param {string} opts.wasmPath  nodblite.wasm 路径
 * @param {Record<string,string>} opts.preopens  宿主目录映射,如 {"/data": "/tmp/db"}
 * @param {boolean} [opts.wasi=true]  是否用 WASI(Node 内置 node:wasi);浏览器需自行注入
 * @param {object} [opts.metrics]  传入一个对象则启用 WASI 调用计量(见 {@link wasiMetrics})
 * @returns {Promise<object>} 实例导出
 */
export async function loadWasm({ wasmPath, preopens, wasi = true, metrics = null }) {
  const fs = await import('node:fs/promises');
  const buf = await fs.readFile(wasmPath);
  const module = await WebAssembly.compile(buf);

  let importObject = {};
  let wasiInterface = null;
  if (wasi) {
    const { WASI } = await import('node:wasi');
    wasiInterface = new WASI({
      version: 'preview1',
      args: [],
      env: {},
      preopens,
    });
    importObject = wasiInterface.getImportObject();
    if (metrics) wrapWasiImports(importObject, metrics);
  }

  const instance = await WebAssembly.instantiate(module, importObject);
  currentMemory = instance.exports.memory ?? null;
  // 库模式(_start 不存在)用 initialize;CLI 模式用 start
  if (wasiInterface) {
    if (typeof instance.exports._start === 'function') {
      wasiInterface.start(instance);
    } else {
      wasiInterface.initialize(instance);
    }
  }
  return instance.exports;
}

/** 当前实例的线性内存(供 WASI 计量读取 nwritten/nread 指针用)。 */
let currentMemory = null;

/**
 * 创建一个 WASI 调用计量对象(传给 {@link loadWasm} 的 `metrics`)。
 *
 * 只统计与数据库 I/O 直接相关的调用:每次调用的**次数**、**字节数**与**累计耗时**。
 * 用途:定位 wasm 写性能瓶颈在「宿主边界往返」还是「CPU 解析」。
 *
 * @returns {{calls: Record<string,number>, bytes: Record<string,number>, nanos: Record<string,number>, reset(): void}}
 */
export function wasiMetrics() {
  const m = {
    calls: {},
    bytes: {},
    nanos: {},
    reset() {
      for (const k of Object.keys(this.calls)) delete this.calls[k];
      for (const k of Object.keys(this.bytes)) delete this.bytes[k];
      for (const k of Object.keys(this.nanos)) delete this.nanos[k];
    },
  };
  return m;
}

/**
 * 包装 importObject 里的 WASI 函数,记录调用次数与字节数。
 *
 * `fd_pwrite(fd, iovs, iovs_len, offset, *nwritten)` 的字节数在调用结束后
 * 由 wasm 写回 `*nwritten`,所以直接从该指针读。
 */
function wrapWasiImports(importObject, metrics) {
  const wasi = importObject.wasi_snapshot_preview1;
  if (!wasi) return;
  const BYTE_FUNCS = new Set(['fd_pwrite', 'fd_pread', 'fd_write', 'fd_read']);
  for (const name of Object.keys(wasi)) {
    const orig = wasi[name];
    if (typeof orig !== 'function') continue;
    wasi[name] = (...args) => {
      const t0 = process.hrtime.bigint();
      const rc = orig(...args);
      const dt = Number(process.hrtime.bigint() - t0);
      metrics.calls[name] = (metrics.calls[name] ?? 0) + 1;
      metrics.nanos[name] = (metrics.nanos[name] ?? 0) + dt;
      if (BYTE_FUNCS.has(name) && currentMemory) {
        // 最后一个参数是 *nwritten / *nread 指针
        const outPtr = args[args.length - 1];
        if (outPtr) {
          const view = new DataView(currentMemory.buffer);
          metrics.bytes[name] = (metrics.bytes[name] ?? 0) + view.getUint32(outPtr, true);
        }
      }
      return rc;
    };
  }
}

/**
 * 在 wasm 线性内存里放一个 NUL 结尾的 UTF-8 字符串。
 *
 * @param {object} exports wasm 导出
 * @param {string} text
 * @returns {{ptr: number, len: number}} 用完后调用 {@link freeString}
 */
export function allocString(exports, text) {
  // +1 给结尾 NUL;CString 语义要求它存在
  const bytes = new TextEncoder().encode(text + '\0');
  const ptr = exports.nodblite_malloc(bytes.length);
  if (!ptr) throw new Error(`nodblite_malloc(${bytes.length}) 失败`);
  new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
  return { ptr, len: bytes.length };
}

/** 释放 {@link allocString} 分配的缓冲区。 */
export function freeString(exports, { ptr, len }) {
  if (ptr) exports.nodblite_free(ptr, len);
}

/** 读 wasm 线性内存里的 UTF-8 字符串。 */
function readString(exports, ptr, len) {
  if (!ptr || len === 0) return null;
  return new TextDecoder().decode(new Uint8Array(exports.memory.buffer, ptr, len));
}

/**
 * 解码一个 NodbliteResult* 并释放它(arena 语义:只 free 一次)。
 *
 * @returns {{code:number, count:number, affected:number, docs:string[], value:string|null}}
 */
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

/** 读线程局部错误信息。 */
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
  // NodbliteError = { int32 code; Slice code_str; Slice message; Slice op; }
  // wasm32:code@0,code_str@4,message@12,op@20(指针 4 字节 + 4 字节对齐)
  return {
    code,
    codeStr: slice(4),
    message: slice(12),
    op: slice(20),
  };
}

/**
 * 打开数据库并返回便捷封装。
 *
 * WASI 下路径必须是 preopen 目录内的绝对路径,如 preopens {'/data': dir}
 * 时用 `openDb(exports, '/data/app.nodb')`。
 *
 * @param {object} exports wasm 导出
 * @param {string} path wasm 内部路径
 * @param {boolean} [createIfMissing=true]
 * @returns {NodbliteDb}
 */
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

/** 数据库句柄封装。所有方法在失败时抛错(附错误码与中文消息)。 */
export class NodbliteDb {
  constructor(exports, handle) {
    this.exports = exports;
    this.handle = handle;
  }

  /** 调用一个 `(db, coll, a, b, c, ...)` 形态的导出,自动管理字符串生命周期。
   *  `strings` 中的 null/undefined 会作为空指针(0)占位,不省略位置。 */
  #call(fn, coll, strings = [], nums = []) {
    const strs = [coll, ...strings].map((s) =>
      s === null || s === undefined ? null : s,
    );
    const allocs = strs.filter((s) => s !== null).map((s) => allocString(this.exports, s));
    let allocIdx = 0;
    try {
      const args = [this.handle, ...strs.map((s) => (s === null ? 0 : allocs[allocIdx++].ptr)), ...nums];
      const resultPtr = fn(...args);
      const result = readResult(this.exports, resultPtr);
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
    const r = this.#call(
      this.exports.nodblite_insert,
      collection,
      [JSON.stringify(doc)],
    );
    return r.value; // _id 裸字符串
  }

  insertMany(collection, docs) {
    const r = this.#call(
      this.exports.nodblite_insert_many,
      collection,
      [JSON.stringify(docs)],
    );
    // value 为 ids 的 JSON 数组字符串
    return JSON.parse(r.value ?? '[]');
  }

  /**
   * 流式批量写入:逐条推入,避免把整个数组序列化成一个大字符串。
   *
   * 内存峰值只与单条文档有关;全部文档仍在 **一个写事务** 内提交,
   * 语义与 {@link insertMany} 一致(任一主键冲突则整批回滚)。
   *
   * @param {string} collection
   * @param {Iterable<object>} docs
   * @returns {string[]} 主键数组(与入参顺序一致)
   */
  bulkInsert(collection, docs) {
    const c = allocString(this.exports, collection);
    const bulk = this.exports.nodblite_bulk_begin(c.ptr);
    freeString(this.exports, c);
    if (!bulk) {
      const err = lastError(this.exports);
      throw new Error(err?.message ?? 'bulk_begin 失败');
    }

    try {
      for (const doc of docs) {
        const d = allocString(this.exports, JSON.stringify(doc));
        let rc;
        try {
          rc = this.exports.nodblite_bulk_add(bulk, d.ptr);
        } finally {
          freeString(this.exports, d);
        }
        if (rc !== 0) {
          const err = lastError(this.exports);
          const e = new Error(err?.message ?? `bulk_add 失败(code=${rc})`);
          e.code = rc;
          throw e;
        }
      }
    } catch (e) {
      // 任一条非法 → 放弃整个会话(库不受影响)
      this.exports.nodblite_bulk_abort(bulk);
      throw e;
    }

    const resultPtr = this.exports.nodblite_bulk_commit(this.handle, bulk);
    const r = readResult(this.exports, resultPtr);
    if (r.code !== 0) {
      const err = lastError(this.exports);
      const e = new Error(err?.message ?? `bulk_commit 失败(code=${r.code})`);
      e.code = r.code;
      throw e;
    }
    return JSON.parse(r.value ?? '[]');
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

  get(collection, id) {
    const r = this.#call(this.exports.nodblite_get, collection, [id]);
    return r.value ? JSON.parse(r.value) : null;
  }

  update(collection, filter, update) {
    return this.#call(this.exports.nodblite_update, collection, [
      JSON.stringify(filter),
      JSON.stringify(update),
    ]).affected;
  }

  delete(collection, filter) {
    return this.#call(this.exports.nodblite_delete, collection, [
      JSON.stringify(filter),
    ]).affected;
  }

  count(collection, filter = {}) {
    return this.#call(this.exports.nodblite_count, collection, [
      JSON.stringify(filter),
    ]).affected;
  }

  listCollections() {
    // nodblite_list_collections(db) —— 无集合参数
    const resultPtr = this.exports.nodblite_list_collections(this.handle);
    const r = readResult(this.exports, resultPtr);
    if (r.code !== 0) {
      const err = lastError(this.exports);
      const e = new Error(err?.message ?? 'list_collections 失败');
      e.code = r.code;
      throw e;
    }
    return r.docs.map((d) => JSON.parse(d));
  }

  exec(command) {
    const r = this.#call(this.exports.nodblite_exec, null, [JSON.stringify(command)]);
    return r;
  }

  /**
   * 创建单字段索引。
   *
   * @param {string} collection
   * @param {string} name 索引名
   * @param {string} field 字段路径(支持点号)
   * @param {{unique?: boolean, sparse?: boolean}} [opts]
   */
  createIndex(collection, name, field, { unique = false, sparse = false } = {}) {
    this.#call(
      this.exports.nodblite_create_index,
      collection,
      [name, field],
      [unique ? 1 : 0, sparse ? 1 : 0],
    );
  }

  /** 列出集合的索引定义(JSON 数组字符串)。 */
  listIndexes(collection) {
    const r = this.#call(this.exports.nodblite_list_indexes, collection);
    return r.docs.map((d) => JSON.parse(d));
  }

  /** 删除索引。 */
  dropIndex(collection, name) {
    this.#call(this.exports.nodblite_drop_index, collection, [name]);
  }

  close() {
    if (this.handle) {
      this.exports.nodblite_close(this.handle);
      this.handle = null;
    }
  }
}
