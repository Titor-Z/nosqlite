// nodblite 的核心 API —— 同步风格,API 手感对齐 better-sqlite3。
//
// 为什么是同步的:C ABI 本身是同步的,套一层 Promise 只是把同步调用包进
// 微任务,不会带来任何并发收益,反而让调用方多写 await。这与 better-sqlite3
// 的判断一致。

import { loadBindings, sliceToString, resolveLibPath } from './bindings.js';

/** nodblite 错误码 → 错误类名。 */
const ERROR_NAMES = {
  1: 'DuplicateKeyError',
  2: 'InvalidPathError',
  3: 'InvalidFormatError',
  4: 'VersionMismatchError',
  5: 'InvalidCollectionError',
  6: 'InvalidFilterError',
  7: 'InvalidUpdateError',
  8: 'InvalidSortError',
  9: 'InvalidProjectionError',
  10: 'InvalidDocumentError',
  11: 'IoError',
  99: 'InternalError',
};

/** 所有 nodblite 错误的基类。 */
export class NodbliteError extends Error {
  constructor(code, message, op) {
    super(message || `nodblite 错误 ${code}`);
    this.name = ERROR_NAMES[code] || 'NodbliteError';
    this.code = code;
    /** 出错的操作名,如 "insert" / "find"。 */
    this.op = op || '';
  }
}

/** 动态创建具体错误类,便于 `instanceof` 判断。 */
export const Errors = Object.fromEntries(
  Object.values(ERROR_NAMES).map((name) => [
    name,
    class extends NodbliteError {
      constructor(code, message, op) {
        super(code, message, op);
        this.name = name;
      }
    },
  ]),
);

function makeError(code, message, op) {
  const Cls = Errors[ERROR_NAMES[code]] || NodbliteError;
  return new Cls(code, message, op);
}

/**
 * 一个 nodblite 数据库。
 *
 * 用法与 better-sqlite3 类似:
 * ```js
 * import { Database } from 'nodblite';
 * const db = new Database('app.nodb');
 * db.collection('users').insert({ name: '张三' });
 * db.close();
 * ```
 */
export class Database {
  #f;
  #db;
  #closed = false;

  /**
   * @param {string} path 数据库文件路径(必须以 `.nodb` 结尾)
   * @param {{create?: boolean, libPath?: string}} [opts]
   */
  constructor(path, opts = {}) {
    if (!path || typeof path !== 'string') {
      throw new TypeError('Database 需要一个文件路径');
    }
    if (!path.toLowerCase().endsWith('.nodb')) {
      throw new Error(`文件后缀必须是 .nodb,收到: ${path}`);
    }

    this.#f = loadBindings(opts.libPath ?? resolveLibPath());
    this.path = path;
    this.#db = this.#f.open(path, opts.create === false ? 0 : 1);
    if (!this.#db) {
      throw makeError(this.#f.lastErrorCode(), this.#lastError(), 'open');
    }
  }

  #lastError() {
    const p = this.#f.lastError();
    if (!p) return '';
    // lastError 返回 C 字符串指针;koffi 对 const char* 返回值已自动转成 string,
    // 但也可能是裸指针,两种都兼容。
    return typeof p === 'string' ? p : this.#f.koffi.decode(p, 'char');
  }

  #assertOpen() {
    if (this.#closed) throw new Error('数据库已关闭');
  }

  /**
   * 执行一次 C 调用并把结果拷成普通 JS 值。
   *
   * **关键**:`value` / `docs` 指向 arena 内部内存,必须在 `resultFree` 之前
   * 全部拷出来,否则会读到已释放内存(表现为乱码或崩溃)。
   */
  #call(raw) {
    this.#assertOpen();
    if (!raw) throw makeError(this.#f.lastErrorCode(), this.#lastError(), '');

    try {
      const r = this.#f.koffi.decode(raw, this.#f.Result);
      if (r.code !== 0) {
        throw makeError(r.code, this.#lastError(), '');
      }
      const { Slice, koffi } = this.#f;
      return {
        count: Number(r.count),
        affected: Number(r.affected),
        value: sliceToString(koffi, r.value),
        docs:
          r.count === 0
            ? []
            : koffi.decode(r.docs, Slice, r.count).map((s) => sliceToString(koffi, s)),
      };
    } finally {
      this.#f.resultFree(raw);
    }
  }

  // ==================== 读 ====================

  /**
   * 查询文档。
   * @template T
   * @param {string} collection
   * @param {object} [filter] MongoDB 风格过滤条件
   * @param {{sort?: object, fields?: object, skip?: number, limit?: number}} [opts]
   * @returns {T[]}
   */
  find(collection, filter = {}, opts = {}) {
    const r = this.#call(
      this.#f.find(
        this.#db,
        collection,
        JSON.stringify(filter),
        opts.sort ? JSON.stringify(opts.sort) : null,
        opts.fields ? JSON.stringify(opts.fields) : null,
        BigInt(opts.skip ?? 0),
        BigInt(opts.limit ?? 0),
      ),
    );
    return r.docs.map((s) => JSON.parse(s));
  }

  /**
   * 分页查询:一次拿到「匹配总数 + 当页文档」,省掉单独 count。
   * @template T
   * @returns {{total: number, items: T[]}}
   */
  findPage(collection, filter = {}, opts = {}) {
    const r = this.#call(
      this.#f.findPage(
        this.#db,
        collection,
        JSON.stringify(filter),
        opts.sort ? JSON.stringify(opts.sort) : null,
        opts.fields ? JSON.stringify(opts.fields) : null,
        BigInt(opts.skip ?? 0),
        BigInt(opts.limit ?? 0),
        opts.limit == null ? 1 : 0,
      ),
    );
    return { total: r.affected, items: r.docs.map((s) => JSON.parse(s)) };
  }

  /**
   * 按 `_id` 读单条,不存在返回 `null`。
   * @template T
   * @returns {T | null}
   */
  get(collection, id) {
    const r = this.#call(this.#f.get(this.#db, collection, String(id)));
    return r.value === '' ? null : JSON.parse(r.value);
  }

  /** 计数。 */
  count(collection, filter = {}) {
    return this.#call(this.#f.count(this.#db, collection, JSON.stringify(filter))).affected;
  }

  /** 列出全部集合名。 */
  collections() {
    return this.#call(this.#f.listCollections(this.#db)).docs.map((s) => JSON.parse(s));
  }

  // ==================== 写 ====================

  /** 插入一条,返回 `_id`。 */
  insert(collection, doc) {
    return this.#call(this.#f.insert(this.#db, collection, JSON.stringify(doc))).value;
  }

  /** 批量插入(单事务),返回 `_id` 数组。 */
  insertMany(collection, docs) {
    if (!Array.isArray(docs)) throw new TypeError('insertMany 需要数组');
    const r = this.#call(
      this.#f.insertMany(this.#db, collection, JSON.stringify(docs)),
    );
    return JSON.parse(r.value);
  }

  /** 更新,返回实际修改条数。 */
  update(collection, filter, update) {
    return this.#call(
      this.#f.update(this.#db, collection, JSON.stringify(filter), JSON.stringify(update)),
    ).affected;
  }

  /** 删除,返回删除条数。 */
  delete(collection, filter) {
    return this.#call(this.#f.delete(this.#db, collection, JSON.stringify(filter))).affected;
  }

  /**
   * 更新或插入。
   * @returns {{updated: number} | {inserted: string}}
   */
  upsert(collection, filter, update) {
    const r = this.#call(
      this.#f.upsert(this.#db, collection, JSON.stringify(filter), JSON.stringify(update)),
    );
    return JSON.parse(r.value);
  }

  // ==================== 索引 ====================

  /**
   * 创建索引。
   * @param {{unique?: boolean, sparse?: boolean, fields?: string[], directions?: number[]}} [opts]
   *   传 `fields` 则为复合索引;否则用 `field`。
   */
  createIndex(collection, name, field, opts = {}) {
    const unique = opts.unique ? 1 : 0;
    const sparse = opts.sparse ? 1 : 0;

    if (opts.fields && opts.fields.length > 0) {
      const r = this.#call(
        this.#f.createIndexComposite(
          this.#db,
          collection,
          name,
          JSON.stringify(opts.fields),
          opts.directions ? JSON.stringify(opts.directions) : null,
          unique,
          sparse,
        ),
      );
      return r.value;
    }

    const r = this.#call(
      this.#f.createIndex(this.#db, collection, name, field, unique, sparse),
    );
    return r.value;
  }

  /** 列出索引定义。 */
  listIndexes(collection) {
    return JSON.parse(this.#call(this.#f.listIndexes(this.#db, collection)).value);
  }

  /** 删除索引。 */
  dropIndex(collection, name) {
    return this.#call(this.#f.dropIndex(this.#db, collection, name)).value === 'true';
  }

  /**
   * 查看查询计划。
   * @returns {{stage: 'IXSCAN' | 'COLLSCAN', [k: string]: unknown}}
   */
  explain(collection, filter = {}, sort = null) {
    const r = this.#call(
      this.#f.explain(
        this.#db,
        collection,
        JSON.stringify(filter),
        sort ? JSON.stringify(sort) : null,
      ),
    );
    return JSON.parse(r.value);
  }

  // ==================== 生命周期 ====================

  /** 关闭数据库。可重复调用。 */
  close() {
    if (this.#closed) return;
    this.#f.close(this.#db);
    this.#db = null;
    this.#closed = true;
  }

  /** 支持 `using db = new Database(...)` 自动关闭。 */
  [Symbol.dispose]() {
    this.close();
  }
}

export { resolveLibPath };
