# 快速开始:JavaScript / TypeScript

nodblite 是 Rust 写的 C ABI 库,JavaScript **不能直接 `import` 它**,但可以通过 FFI 调用。

有两条路,选哪条取决于你的运行环境:

| 方案 | 适用 | 特点 |
|---|---|---|
| **A. FFI 直连**(koffi) | Node.js | 进程内零拷贝调用,最快;需装一个 npm 包 |
| **B. MongoDB wire 协议** | Node / Deno / Bun / 浏览器 | 只需启动 `nodblite-server`,用官方 `mongodb` 驱动 |

!!! warning "先说结论"
    如果你**已经在用 MongoDB 或想跨运行时兼容** → 走 **B**,几乎零学习成本。
    如果你**追求极致性能、且只在 Node 里用** → 走 **A**。
    两条路的数据格式完全相同(`.nodb` 同一个文件)。

---

## 方案 A:FFI 直连(koffi)

### 为什么是 koffi

| 方案 | 状态 |
|---|---|
| `ffi-napi` | 需要 node-gyp 编译,维护停滞,Node 18+ 常装不上 |
| **`koffi`** | 预编译二进制,`npm install` 即用,**已验证可用** |
| Node 内置 FFI | 不存在 |
| N-API 手写 addon | 要写 C++ 胶水层,不划算 |

### 安装

```bash
npm install koffi
```

### 完整可运行示例

```js
import koffi from 'koffi';

// 1. 加载动态库(路径按你的平台调整)
//    macOS:  libnodblite.dylib
//    Linux:  libnodblite.so
//    Windows: nodblite.dll
const lib = koffi.load('./libnodblite.dylib');

// 2. 声明用到的函数
const open   = lib.func('void* nodblite_open(const char *path, uint8_t create)');
const close  = lib.func('void nodblite_close(void *db)');
const insert = lib.func('void* nodblite_insert(void *db, const char *coll, const char *doc)');
const find   = lib.func(
  'void* nodblite_find(void *db, const char *coll, const char *filter, ' +
  'const char *sort, const char *fields, uint64_t skip, uint64_t limit)'
);
const freeRes = lib.func('void nodblite_result_free(void *r)');

// 3. 声明结果结构(必须与 include/nodblite.h 保持一致)
const Slice = koffi.struct('Slice', {
  ptr: 'uint8_t *',
  len: 'size_t',
});

const Result = koffi.struct('Result', {
  code:     'int32',
  count:    'uint64',
  affected: 'uint64',
  docs:     koffi.pointer(Slice),
  value:    Slice,
});

// 4. Slice → JS 字符串(注意:不保证 NUL 结尾,必须按 len 解码)
const str = (s) =>
  s.len === 0 ? '' : Buffer.from(koffi.decode(s.ptr, 'uint8_t', s.len)).toString('utf8');

// 5. 使用
const db = open('app.nodb', 1);  // 1 = 不存在则创建
if (!db) throw new Error('打开失败');

// 插入
const r = koffi.decode(
  insert(db, 'users', '{"name":"张三","age":30}'),
  Result
);
console.log('插入 id =', str(r.value));
freeRes(r);                      // arena 结果:整块释放一次

// 查询
const res = find(db, 'users', '{"age":{"$gte":18}}', null, null, 0, 0);
const d = koffi.decode(res, Result);

// docs 是长度未知的数组,要按 count 显式解码
const arr = koffi.decode(d.docs, Slice, d.count);
for (const s of arr) {
  console.log(JSON.parse(str(s)));
}
freeRes(res);

close(db);
```

实测输出:

```text
插入 id = "6ad67bdccb2505d1ee000000"
{ name: '张三', age: 30, _id: '6ad67bdccb2505d1ee000000' }
```

### 三个必须注意的点

!!! danger "1. 必须按 `len` 解码,不能当 NUL 结尾字符串"
    `NodbliteSlice` 是 `{ptr, len}`。文档 JSON 内部可能含 `\0`(虽然罕见),按 C 字符串读会截断。

    ```js
    // ✅ 正确
    Buffer.from(koffi.decode(s.ptr, 'uint8_t', s.len)).toString('utf8')
    // ❌ 错误:可能在 \0 处截断
    koffi.decode(s.ptr, 'char')
    ```

!!! danger "2. 结果必须 `nodblite_result_free` 释放"
    每个返回 `void*` 的函数都分配了一块 arena。**不释放就是内存泄漏。**

    好消息:每个结果只需**释放一次**,内部的字符串都跟着一起回收,不用逐字段 free。

!!! danger "3. 结构体布局必须与头文件完全一致"
    写错字段顺序或大小 → 读到垃圾数据甚至崩溃。改头文件后必须同步改 JS 里的 `koffi.struct`。

    建议:把结构声明集中在一个文件里,并在注释中标明对应的头文件版本。

### 封装建议

上面的调用是原始 FFI,实际项目建议包一层:

```js
export class Nodblite {
  #db;
  #lib;

  constructor(path, { create = true } = {}) {
    this.#lib = loadLibrary();
    this.#db = this.#lib.open(path, create ? 1 : 0);
    if (!this.#db) throw new Error(this.lastError());
  }

  insert(coll, doc) {
    const r = this.#decode(this.#lib.insert(this.#db, coll, JSON.stringify(doc)));
    try {
      this.#check(r.code);
      return JSON.parse(this.#str(r.value));
    } finally {
      this.#lib.freeRes(r.__ptr);
    }
  }

  find(coll, filter = {}, opts = {}) {
    const raw = this.#lib.find(
      this.#db, coll,
      JSON.stringify(filter),
      opts.sort ? JSON.stringify(opts.sort) : null,
      opts.fields ? JSON.stringify(opts.fields) : null,
      BigInt(opts.skip ?? 0),
      BigInt(opts.limit ?? 0),
    );
    const r = this.#decode(raw);
    try {
      this.#check(r.code);
      if (r.count === 0) return [];
      return koffi.decode(r.docs, Slice, r.count)
        .map((s) => JSON.parse(this.#str(s)));
    } finally {
      this.#lib.freeRes(raw);
    }
  }
}
```

---

## 方案 B:MongoDB wire 协议(推荐大多数场景)

启动服务端,用官方驱动 —— **不用碰 FFI,不用管内存,跨运行时都能用**。

```bash
nodblite-server --addr 127.0.0.1:27017 app.nodb
```

```js
import { MongoClient } from 'mongodb';

const client = new MongoClient('mongodb://127.0.0.1:27017');
await client.connect();

const users = client.db('app').collection('users');
await users.insertOne({ name: '张三', age: 30 });

const list = await users.find({ age: { $gte: 18 } }).toArray();
await client.close();
```

Deno / Bun 同样可用(它们都能跑 npm 的 `mongodb` 驱动)。

!!! tip "为什么推荐大多数场景用 B"
    | | A. FFI | B. wire 协议 |
    |---|---|---|
    | 性能 | 进程内,最快 | 多一次网络往返(本机约 0.1 ms) |
    | 依赖 | 需 `koffi` + 手动声明结构 | 只需官方驱动 |
    | 内存管理 | 要手动 `result_free` | 驱动托管 |
    | 跨运行时 | 仅 Node | Node / Deno / Bun / 浏览器 |
    | 多进程共享 | ❌ 单文件独占 | ✅ 服务端统一访问 |
    | 升级维护 | 头文件变了要同步改 JS | 协议稳定,不用改 |

---

## 三种语言怎么选

| 语言 | 推荐方式 | 理由 |
|---|---|---|
| **Rust** | 直接 `cargo add nodblite` | 原生,零开销 |
| **C / C++** | `#include <nodblite.h>` | 原生 ABI |
| **Python** | `ctypes` 或 wire 协议 | ctypes 免编译,仓库有示例 |
| **Node.js** | wire 协议(优先)/ koffi | 见上方对比 |
| **Go** | wire 协议 | 无 cgo,官方驱动已验证 |
| **Deno / Bun / 浏览器** | 只能 wire 协议 | 无 FFI 或受沙箱限制 |

---

## 常见问题

??? question "能不能把 C 库编译成 WebAssembly 给浏览器用?"
    技术上可以(Rust → wasm32),但**当前没有做**。障碍是 wasm 没有文件系统,需要把整个数据库放进线性内存,还要重做 redb 的 IO 层。短期内浏览器场景请用 wire 协议 + 后端服务。

??? question "koffi 在 Electron 里能用吗?"
    能。Electron 主进程是 Node 环境。注意打包时要把 `.dylib`/`.so`/`.dll` 作为 extraResource 一起打进去,并按 `process.resourcesPath` 计算路径。

??? question "FFI 方式能多进程同时写吗?"
    不能。`.nodb` 由进程独占(SQLite 同款约束)。需要多进程共享请用方案 B。
