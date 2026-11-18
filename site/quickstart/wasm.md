# wasm:浏览器 / 边缘运行时

同一份 Rust 源码编译到 `wasm32-wasip1`,产出**一个 `.wasm` 文件**(约 2.5 MB,gzip 后约 790 KB),任何有 WASI 运行时的宿主都能跑 —— 不必为每个平台各分发一份动态库。

::: tip 什么时候该用 wasm
- 应用要跑在**浏览器**里,数据留在用户机器上
- 分发一个**单文件**工具,不想按平台挑动态库
- 目标环境只支持 WebAssembly(边缘函数、插件沙箱)

Node.js 里**不需要** wasm —— 用 [npm 包](/quickstart/javascript)更快、更省事。
:::

`.nodb` 文件与原生完全互通:wasm 写的库原生能读,反之亦然。

---

## 获取产物

**Release 包**:GitHub Releases 的 `nodblite-wasm.tar.gz`,内含

```text
nodblite.wasm      本体的 WASI 模块
nodblite.js        Node 宿主封装(零依赖)
include/           C ABI 头文件(自己写宿主时对照)
SHA256SUMS
```

**本地构建**:

```bash
rustup target add wasm32-wasip1
cargo build --release --lib --target wasm32-wasip1
# → target/wasm32-wasip1/release/nodblite.wasm
```

---

## Node 里跑(Node 18+)

`nodblite.js` 用内置的 `node:wasi` 自动装配运行时,并负责在宿主与线性内存之间搬运字符串:

```js
import { loadWasm, openDb } from './nodblite.js';

const exports = await loadWasm({
  wasmPath: './nodblite.wasm',
  preopens: { '/data': './db' },   // 宿主目录 → WASI 里的 /data
});

const db = openDb(exports, '/data/app.nodb');   // 不存在则创建

db.insert('users', { name: '张三', age: 30 });
db.insertMany('users', [{ name: '李四', age: 22 }]);

const adults = db.find('users', { age: { $gte: 18 } }, { sort: { age: -1 }, limit: 20 });
const n = db.count('users');
const total = db.get('users', id);

db.close();
```

`preopens` 是**唯一**的目录映射:wasm 只能看见你映射进去的路径。

---

## 浏览器里跑

浏览器没有 `node:wasi`,需要宿主自己提供 WASI 实现。仓库里有一份零依赖的最小实现可直接复用:

| 文件 | 作用 |
|---|---|
| `studio-ui/src/lib/wasi.js` | `MemoryFs`(内存文件系统)+ `createWasi(fs)`,实现 wasm 用到的 19 个 `wasi_snapshot_preview1` 导入 |
| `studio-ui/src/lib/nodblite.js` | 线性内存字符串搬运 + `NodbliteResult` 解码 |
| `studio-ui/src/lib/db.js` | 会话封装:把 `.nodb` 字节写进内存 FS、打开、导出 |

骨架:

```js
import { MemoryFs, createWasi } from './wasi.js';
import { openDb } from './nodblite.js';

const fs = new MemoryFs();
// 用户拖入/选择的文件:导入就等于把字节写进内存 FS
if (fileBytes?.byteLength) fs.setFile('app.nodb', new Uint8Array(fileBytes));

const wasi = createWasi(fs);
const bytes = await (await fetch('/nodblite.wasm')).arrayBuffer();
const instance = await WebAssembly.instantiate(await WebAssembly.compile(bytes), wasi.imports);
wasi.bind(instance.exports.memory);          // 必须在任何 WASI 调用之前绑定内存

const db = openDb(instance.exports, '/data/app.nodb', true);

// 之后与 Node 一致
db.insert('users', { name: '张三' });
// 导出:读回整个文件的字节
const out = fs.getFile('app.nodb');
```

::: warning repr(C) 布局在 wasm32 上不同
指针是 **4 字节**。`NodbliteResult` 在 wasm32 上是 `code@0 / count@8 / affected@16 / docs@24 / value.ptr@28 / value.len@32`(64 位是 `@0/@8/@16/@24/@32/@40`)。读错偏移的表现是长度读出 `0xFFFFFFFF` → `RangeError: Invalid typed array length`。
:::

---

## API

`NodbliteDb` 的方法与 npm 包同名,但**少几个**(wasm 侧只暴露了基础面):

| 方法 | 说明 |
|---|---|
| `insert(collection, doc)` | 返回 `_id` |
| `insertMany(collection, docs)` | 单事务,返回 `_id` 数组 |
| `bulkInsert(collection, iterable)` | 流式批量写入:逐条推入,内存峰值只与单条有关 |
| `find(collection, filter, { sort, fields, skip, limit })` | 返回文档数组 |
| `get(collection, id)` | 按 `_id` 读单条,不存在返回 `null` |
| `update(collection, filter, update)` | 返回修改条数 |
| `delete(collection, filter)` | 返回删除条数 |
| `count(collection, filter)` | 计数 |
| `listCollections()` | 集合名数组 |
| `createIndex(collection, name, field, { unique, sparse })` | 单字段索引 |
| `listIndexes(collection)` / `dropIndex(collection, name)` | 索引管理 |
| `exec(command)` | 原始命令兜底 |
| `close()` | 关闭 |

与 npm 包的差异:只有 `exec()` 兜底入口不同;数据面方法(`find` / `findPage` / `get` /
`insert` / `insertMany` / `bulkInsert` / `update`(含 `arrayFilters`)/ `upsert` /
`delete` / `count` / `aggregate` / `explain` / 索引 / `cursor` 流式游标 /
`dropCollection` / `compact`)全部对齐。

> 浏览器里没有账号体系 —— wasm 宿主拿到的就是本地文件句柄,认证这层的意义在网络边界,
> 见 [账号与认证](/guide/auth)。

每次调用失败都会抛 `Error`,带 `code`(数字)与 `codeStr`(稳定错误码)。

---

## 性能

同一台机器、同一份数据、`.nodb` 双向互读已实测(20000 条):

| 指标 | 原生 | wasm | 比值 |
|---|---|---|---|
| 读(全量 find) | 0.940 s | 0.949 s | **1.01x** |
| 写(insertMany 单事务) | 基准 | — | **≈1.3x 慢**(min 判据) |
| `fd_pwrite` 页写 | — | 20 ms / 2241 次(1.9%) | — |
| `fd_sync` 落盘 | — | 占 60–90% | — |

写路径的数字必须用 **min 或 p10** 作判据:同一文件、同样脏页,连续 30 次 `fsync` 的耗时是 min 21 ms / max 1140 ms(**54 倍方差**),中位数与单次测量都会被系统负载污染。

结论:**读几乎无损,写慢的地方是 fsync 而不是 WASI 往返**。`fsync` 成本主要来自文件增长(边写边扩容要落元数据),批量写入摊薄提交次数是唯一有效手段 —— 用 `insertMany` 或 `bulkInsert`,不要逐条 `insert`。

完整方法学见[性能报告](/reference/performance)。
