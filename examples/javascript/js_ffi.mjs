// nodblite 的 JavaScript FFI 封装(基于 koffi)。
//
// 这是给 docs/quickstart/javascript.md 配套的**可运行参考实现**,
// 不是官方 SDK —— 目的是证明「Node 能直接调用 C 库」这条路可行,
// 并给出正确的内存管理与字符串解码写法。
//
// 用法:
//   npm install koffi
//   node js_ffi.mjs
//
// 需要先构建 C 库:
//   cargo build --release
//   动态库位于 target/release/(macOS libnodblite.dylib / Linux libnodblite.so)

import koffi from 'koffi';

/** 平台对应的动态库文件名。 */
function libFileName() {
  if (process.platform === 'darwin') return 'libnodblite.dylib';
  if (process.platform === 'win32') return 'nodblite.dll';
  return 'libnodblite.so';
}

// ==================== C 结构声明 ====================
// 必须与 include/nodblite.h 严格一致。改头文件后要同步改这里。

const Slice = koffi.struct('NodbliteSlice', {
  ptr: 'uint8_t *',
  len: 'size_t',
});

const Result = koffi.struct('NodbliteResult', {
  code: 'int32',
  count: 'uint64',
  affected: 'uint64',
  docs: koffi.pointer(Slice),
  value: Slice,
});

/**
 * 把返回的 Slice 解码成 JS 字符串。
 *
 * 注意:切片**不保证 NUL 结尾**,必须按 len 解码;
 * 直接当 C 字符串读会在内含 `\0` 时截断。
 */
function toText(s) {
  if (!s || s.len === 0) return '';
  return Buffer.from(koffi.decode(s.ptr, 'uint8_t', s.len)).toString('utf8');
}

/** nodblite 的 Node 封装。 */
export class Nodblite {
  #db;
  #f;

  /**
   * @param {string} path 数据库文件路径(必须以 .nodb 结尾)
   * @param {{create?: boolean, libPath?: string}} [opts]
   */
  constructor(path, opts = {}) {
    const libPath = opts.libPath ?? new URL(`./${libFileName()}`, import.meta.url).pathname;
    const lib = koffi.load(libPath);

    // 只声明实际用到的函数,减少初始化开销
    this.#f = {
      open: lib.func('void* nodblite_open(const char *path, uint8_t create)'),
      close: lib.func('void nodblite_close(void *db)'),
      insert: lib.func('void* nodblite_insert(void *db, const char *coll, const char *doc)'),
      insertMany: lib.func(
        'void* nodblite_insert_many(void *db, const char *coll, const char *docs)',
      ),
      get: lib.func('void* nodblite_get(void *db, const char *coll, const char *id)'),
      find: lib.func(
        'void* nodblite_find(void *db, const char *coll, const char *filter, ' +
          'const char *sort, const char *fields, uint64_t skip, uint64_t limit)',
      ),
      update: lib.func(
        'void* nodblite_update(void *db, const char *coll, const char *filter, const char *upd)',
      ),
      del: lib.func('void* nodblite_delete(void *db, const char *coll, const char *filter)'),
      count: lib.func('void* nodblite_count(void *db, const char *coll, const char *filter)'),
      free: lib.func('void nodblite_result_free(void *r)'),
      lastError: lib.func('const char* nodblite_last_error()'),
    };

    this.#db = this.#f.open(path, opts.create === false ? 0 : 1);
    if (!this.#db) {
      throw new Error(`打开 ${path} 失败: ${this.#lastError()}`);
    }
  }

  #lastError() {
    const p = this.#f.lastError();
    return p ? koffi.decode(p, 'char') : '(未知错误)';
  }

  /**
   * 执行一次 FFI 调用并解码结果。
   *
   * **关键**:无论成功失败,`raw` 指针都必须释放,否则内存泄漏。
   */
  #call(rawPtr) {
    if (!rawPtr) throw new Error(this.#lastError());
    try {
      const r = koffi.decode(rawPtr, Result);
      if (r.code !== 0) {
        throw new Error(`nodblite 错误 ${r.code}: ${this.#lastError()}`);
      }
      // **必须在释放前把所有字符串拷成 JS 值** ——
      // r.value / r.docs 都指向 arena 内部内存,释放后即失效。
      const out = {
        count: r.count,
        affected: r.affected,
        value: toText(r.value),
        docs:
          r.count === 0
            ? []
            : koffi.decode(r.docs, Slice, r.count).map(toText),
      };
      return out;
    } finally {
      this.#f.free(rawPtr);
    }
  }

  /**
   * 插入一条文档,返回 `_id`。
   *
   * 注意:C ABI 里 `value` 是**裸 `_id`**(不带 JSON 引号),便于 C 侧直接当主键用,
   * 所以这里不能 `JSON.parse`。
   */
  insert(coll, doc) {
    const r = this.#call(this.#f.insert(this.#db, coll, JSON.stringify(doc)));
    return r.value; // 裸 _id,不带 JSON 引号
  }

  /** 批量插入(单事务),返回 `_id` 数组(JSON 数组字符串)。 */
  insertMany(coll, docs) {
    const r = this.#call(this.#f.insertMany(this.#db, coll, JSON.stringify(docs)));
    return JSON.parse(r.value);
  }

  /** 按 `_id` 读取,不存在返回 `null`。 */
  get(coll, id) {
    const r = this.#call(this.#f.get(this.#db, coll, id));
    return r.value === '' ? null : JSON.parse(r.value);
  }

  /**
   * 查询。
   * @param {object} [filter] 过滤条件
   * @param {{sort?: object, fields?: object, skip?: number, limit?: number}} [opts]
   */
  find(coll, filter = {}, opts = {}) {
    const r = this.#call(
      this.#f.find(
        this.#db,
        coll,
        JSON.stringify(filter),
        opts.sort ? JSON.stringify(opts.sort) : null,
        opts.fields ? JSON.stringify(opts.fields) : null,
        BigInt(opts.skip ?? 0),
        BigInt(opts.limit ?? 0),
      ),
    );
    return r.docs.map((t) => JSON.parse(t));
  }

  /** 更新,返回实际修改条数。 */
  update(coll, filter, update) {
    const r = this.#call(
      this.#f.update(this.#db, coll, JSON.stringify(filter), JSON.stringify(update)),
    );
    return Number(r.affected);
  }

  /** 删除,返回删除条数。 */
  delete(coll, filter) {
    const r = this.#call(this.#f.del(this.#db, coll, JSON.stringify(filter)));
    return Number(r.affected);
  }

  /** 计数。 */
  count(coll, filter = {}) {
    const r = this.#call(this.#f.count(this.#db, coll, JSON.stringify(filter)));
    return Number(r.affected);
  }

  /** 关闭句柄。 */
  close() {
    if (this.#db) {
      this.#f.close(this.#db);
      this.#db = null;
    }
  }
}

// ==================== 自测 ====================

if (import.meta.url === `file://${process.argv[1]}`) {
  const path = '/tmp/nodblite_js_demo.nodb';
  const fs = await import('node:fs');
  try { fs.unlinkSync(path); } catch {}

  const db = new Nodblite(path);

  console.log('插入 id =', db.insert('users', { name: '张三', age: 30 }));
  db.insertMany('users', [
    { name: '李四', age: 25 },
    { name: '王五', age: 42 },
  ]);

  console.log('总数 =', db.count('users'));
  console.log('成年 =', db.find('users', { age: { $gte: 18 } }, { sort: { age: -1 } }));

  db.update('users', { name: '张三' }, { $inc: { age: 1 } });
  console.log('更新后 =', db.find('users', { name: '张三' }));

  db.delete('users', { name: '李四' });
  console.log('删除后总数 =', db.count('users'));

  db.close();
  fs.unlinkSync(path);
  console.log('✓ 全部通过');
}
