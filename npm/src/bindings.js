// nodblite 的 koffi 绑定层。
//
// 本文件只做「C ABI ↔ JS」的机械映射,不含业务逻辑。
// 所有函数签名必须与 include/nodblite.h 严格一致 —— 改头文件后要同步这里。

import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

/** 各平台动态库文件名。 */
const LIB_NAMES = {
  darwin: 'libnodblite.dylib',
  linux: 'libnodblite.so',
  win32: 'nodblite.dll',
};

/**
 * 定位动态库。
 *
 * 查找顺序:
 * 1. 环境变量 `NODBLITE_LIB`(显式指定,最高优先级)
 * 2. 包内 `prebuilds/<platform>-<arch>/`
 * 3. 包内 `prebuilds/`
 * 4. 开发场景:`../target/release/` 与 `../target/debug/`
 */
export function resolveLibPath() {
  if (process.env.NODBLITE_LIB) {
    if (!existsSync(process.env.NODBLITE_LIB)) {
      throw new Error(`NODBLITE_LIB 指向的文件不存在: ${process.env.NODBLITE_LIB}`);
    }
    return process.env.NODBLITE_LIB;
  }

  const name = LIB_NAMES[process.platform];
  if (!name) {
    throw new Error(`不支持的平台: ${process.platform}(支持 darwin / linux / win32)`);
  }

  const candidates = [
    join(__dirname, '..', 'prebuilds', `${process.platform}-${process.arch}`, name),
    join(__dirname, '..', 'prebuilds', name),
    // 源码仓库内直接使用时
    join(__dirname, '..', '..', 'target', 'release', name),
    join(__dirname, '..', '..', 'target', 'debug', name),
  ];

  for (const p of candidates) {
    if (existsSync(p)) return p;
  }

  throw new Error(
    `找不到 nodblite 动态库(${name})。\n` +
      `已尝试:\n${candidates.map((c) => `  - ${c}`).join('\n')}\n\n` +
      `解决办法:\n` +
      `  1. 在源码仓库执行 cargo build --release,或\n` +
      `  2. 设置环境变量 NODBLITE_LIB=/path/to/${name}\n` +
      `  3. 从 https://github.com/Titor-Z/nosqlite/releases 下载 FFI 包`,
  );
}

// koffi 的结构类型注册表是**全局**的:`koffi.struct()` 同名只能调一次,
// 否则抛 "Duplicate type name"。所以这里缓存起来,进程内只声明一次。
let cachedKoffi = null;
let cachedTypes = null;

/** 拿到 koffi 与只声明一次的结构类型。 */
function getKoffiAndTypes() {
  if (cachedKoffi) return { koffi: cachedKoffi, ...cachedTypes };

  const koffi = require('koffi');

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

  cachedKoffi = koffi;
  cachedTypes = { Slice, Result };
  return { koffi, Slice, Result };
}

/** 加载 koffi 并声明所有用到的 C 函数与结构。 */
export function loadBindings(libPath = resolveLibPath()) {
  const { koffi, Slice, Result } = getKoffiAndTypes();
  const lib = koffi.load(libPath);

  // ---------- 函数 ----------

  const fn = (sig) => lib.func(sig);

  return {
    koffi,
    Slice,
    Result,

    open: fn('void *nodblite_open(const char *path, uint8_t create_if_missing)'),
    close: fn('void nodblite_close(void *db)'),

    insert: fn('void *nodblite_insert(void *db, const char *coll, const char *doc)'),
    insertMany: fn('void *nodblite_insert_many(void *db, const char *coll, const char *docs)'),
    get: fn('void *nodblite_get(void *db, const char *coll, const char *id)'),
    update: fn(
      'void *nodblite_update(void *db, const char *coll, const char *filter, const char *upd)',
    ),
    delete: fn('void *nodblite_delete(void *db, const char *coll, const char *filter)'),
    upsert: fn(
      'void *nodblite_upsert(void *db, const char *coll, const char *filter, const char *upd)',
    ),
    count: fn('void *nodblite_count(void *db, const char *coll, const char *filter)'),
    find: fn(
      'void *nodblite_find(void *db, const char *coll, const char *filter, ' +
        'const char *sort, const char *fields, uint64_t skip, uint64_t limit)',
    ),
    findPage: fn(
      'void *nodblite_find_page(void *db, const char *coll, const char *filter, ' +
        'const char *sort, const char *fields, uint64_t skip, uint64_t limit, ' +
        'uint8_t limit_is_null)',
    ),
    listCollections: fn('void *nodblite_list_collections(void *db)'),

    createIndex: fn(
      'void *nodblite_create_index(void *db, const char *coll, const char *name, ' +
        'const char *field, uint8_t unique, uint8_t sparse)',
    ),
    createIndexComposite: fn(
      'void *nodblite_create_index_composite(void *db, const char *coll, const char *name, ' +
        'const char *fields, const char *dirs, uint8_t unique, uint8_t sparse)',
    ),
    listIndexes: fn('void *nodblite_list_indexes(void *db, const char *coll)'),
    dropIndex: fn('void *nodblite_drop_index(void *db, const char *coll, const char *name)'),

    explain: fn(
      'void *nodblite_explain(void *db, const char *coll, const char *filter, ' +
        'const char *sort)',
    ),

    cursorNext: fn('void *nodblite_cursor_next(void *cur)'),
    cursorClose: fn('void nodblite_cursor_close(void *cur)'),

    resultFree: fn('void nodblite_result_free(void *r)'),
    lastError: fn('const char *nodblite_last_error()'),
    lastErrorCode: fn('int32 nodblite_last_error_code()'),
  };
}

/** 把 C 的 `NodbliteSlice` 解成 JS 字符串。 */
export function sliceToString(koffi, s) {
  if (!s || s.len === 0) return '';
  return Buffer.from(koffi.decode(s.ptr, 'uint8_t', s.len)).toString('utf8');
}
