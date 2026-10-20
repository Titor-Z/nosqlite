# Changelog

版本号采用语义版本。完整设计与决策记录见 [AGENTS.md](./AGENTS.md) 的 `taolun` 章节。

## [0.6.9] - 2026-10-21

补 macOS x64 预编译库。

### 修复
- **npm 包缺 macOS x64 库**:`macos-latest` runner 只产出 arm64,Intel Mac / Rosetta 用户找不到 `darwin-x64`。npm 工作流补 `x86_64-apple-darwin` 交叉构建,预编译目录从 3 个增至 4 个。

## [0.6.8] - 2026-10-21

修掉 npm 包的加载入口。

### 修复
- **npm 包 `require('nodblite')` 失败**:`package.json` 的 `main` / `exports.require` 指向 `dist/index.cjs`,但 `tsc` 只产出 `dist/index.js`,导致 `Cannot find module .../dist/index.cjs`。已改为纯 ESM(`"type": "module"` 本就如此),CJS 项目改用动态 `import()`。
  - 曾尝试用第二份 tsconfig 产出 CJS,失败:`bindings.js` 使用 `import.meta.url`,转 CJS 后与 Node 注入的 `require` 冲突(`SyntaxError: Identifier 'require' has already been declared`)。
- `repository.url` 改写为 npm 规范化后的 `git+https://` 形式,消掉发布警告。

## [0.6.7] - 2026-10-21

WASM 形态落地 + 写入路径实测优化。

### 新增
- **WASM 产物(`wasm32-wasip1`)**:同一份源码编译出单个 `.wasm`(2.5 MB,gzip 后 790 KB),覆盖所有平台所有语言,不再需要为每个语言/平台各分发一份 C 动态库。`.nodb` 文件与原生完全互通(双向读写验证)。
- **`web/nodblite.js`**:零依赖 JS/Node 宿主封装(WASI preopen + 线性内存字符串 + `NodbliteResult`/`NodbliteError` wasm32 布局解码)。
- **`nodblite_malloc` / `nodblite_free`**:供 wasm 宿主在线性内存里传字符串(宿主拿不到 Rust 分配器)。
- **流式批量写入会话 `nodblite_bulk_begin/add/commit/abort`**:宿主可按条推进,不必先把整个数组序列化成一个大字符串;全部文档仍在单个写事务内提交,语义与 `insert_many` 一致(任一主键冲突整批回滚)。JS 侧对应 `Database#bulkInsert()`。
  - 实测 JS 堆增量:10.6 MB → **2.6 MB**(4x 降低)。
- **`Document::from_value(Value)`**:消掉 4 处「JSON → Value → String → 再解析」的往返(实测 66.9 ms → 29.1 ms,2.3x)。
- **`web/bench-write.mjs` + `web/wasiMetrics()`**:WASI 调用计量(次数/字节/耗时),用于写路径归因。
- **`examples/bench_wasm_compare.rs`**:原生侧 `insert_many` 对照基准。
- `release.yml` 新增 `wasm` job;`ci.yml` 新增 wasm 构建与冒烟验证。

### 修复
- **`ObjectId::generate` 在 WASI 下 panic**:`std::process::id()` 在 wasm 上不可用(`no pids on this platform`),导致插入直接崩溃。改为非 wasm 才取 PID。

### 实测结论(详见 AGENTS.md 认知修正)
- **wasm 的 `fd_sync` 与原生 `sync_all` 等价**(22.5 ms vs 21.9 ms,1.03x)—— 早前「wasm 写慢 3.6x / WASI 慢 26x」的结论是 fsync 方差污染造成的假象。
- **`fd_pwrite` 不是瓶颈**:20000 条 / 9 MB 下 2241 次页写合计仅 20 ms(1.9%),`fd_sync` 占 60–90%。
- **fsync 的单次测量不可用**:同文件同脏页连续 30 次 fsync,min 21 ms / max 1140 ms(**54x 方差**),写基准必须用 min/p10 判据。
- **页写合并与 `Durability::None` 折中经数据否决**,不做。

## [0.6.6] - 2026-10-19

JavaScript 生态接入。

### 新增
- **npm 包 `nodblite`**:TypeScript 类型定义 + 通过 [koffi](https://www.npmjs.com/package/koffi) 直连 C ABI,同步风格 API(手感对齐 better-sqlite3),无需 node-gyp / Python / C++ 编译器。
- **C ABI 扩展**:索引相关(`create_index` / `create_index_composite` / `list_indexes` / `drop_index`)、`explain`、`find_page`、`get`、`upsert`。
- 文档站新增 JavaScript 使用指南;`examples/javascript/js_ffi.mjs` 可运行示例。

## [0.6.5] - 2026-10-19

### 新增
- **文档站**(GitHub Pages + MkDocs Material,14 页):快速上手(Rust / C ABI / CLI / wire 驱动四条入口)、查询/更新/投影/索引/聚合指南、**与 MongoDB 的差异**、性能、FAQ。`--strict` 构建零警告。

## [0.6.4] - 2026-10-19

### 变更(破坏性)
- **改名 `nosqlite` → `nodblite`**:crates.io 的 `nosqlite` 已被同名同定位的项目占用。只改对外标识(crate/二进制/C ABI 函数/头文件/wire 命名空间/文档),**存储格式与 `.nodb` 后缀不变**;用 git worktree 跨版本验证旧库可读。

## [0.6.3] - 2026-09-16

性能实测与一处真实缺陷修复。

### 修复
- **唯一索引建索引退化为 O(n²)**:唯一性前置检查用 `Vec::contains` 做线性查找,10 万条唯一值要 74 秒(非唯一索引仅 2.6 秒)。改用 `HashSet` 后降至 2.4 秒(31 倍),并补回归测试防止再退化。
- **CI 上 C ABI 测试失败**:`cargo test --all-targets` 不构建 cdylib,干净环境下动态库缺失。改为测试按需执行 `cargo build --lib`。

### 新增
- `examples/bench_index.rs`:索引性能基准(查询加速比、建索引代价、索引对写入的拖累)。
- README 新增「索引效果」小节,含实测数据与选型提示。

## [0.6.2] - 2026-09-16

工程化收尾:Node.js 驱动验收、崩溃恢复测试、聚合分组修复。

### 新增
- **Node.js 官方驱动验收**(`tests/node_compat/`):用官方 `mongodb` npm 包走完整 wire 协议栈,12 项端到端用例(握手/CRUD/游标分批/数组操作符/过滤操作符/投影/索引/聚合)。
- **崩溃恢复与文件一致性测试**(`tests/crash_recovery.rs`,7 例):
  - 真实进程 `abort` 后验证「单事务写入要么全在、要么一条没有」;
  - 多次「写入 → abort → 重开」循环不累积损坏;
  - 截断 / 字节翻转 / 随机垃圾 / 空文件一律明确报错;
  - 崩溃后重建索引,与全表扫结果一致。
- **`$group` 真正的分组聚合**:支持 `_id` 为常量或 `"$字段"`,累加器 `$sum`(常量或字段路径)与 `$count`。

### 修复
- **截断文件导致进程 panic**:redb 3.1.3 在文件被截断时走 `assert!` 而非返回错误。新增打开前预检 —— 解析 redb 头部声明的文件总长度并与实际长度比对,把崩溃变成明确的 `InvalidFormat` 错误。

### 工程
- `Cargo.toml` 增加 `exclude`,测试用 Node 依赖不打进发布包(93 个文件 / 280 KiB 压缩)。
- `cargo publish --dry-run` 通过。

## [0.6.1] - 2026-09-16

数组操作与投影位置运算符。

### 新增
- **`$pull`**:三种匹配形态 —— 标量等值、查询操作符(`$in`/`$gt` 等)、**对象子集匹配**(`{"$pull": {"items": {"score": 8}}}` 移除所有 `score == 8` 的对象)。
- **`$pop`**:`1` 移除末尾、`-1` 移除开头;空数组与缺失字段无操作。
- **投影位置运算符 `{"arr.$": 1}`**:只返回 filter 命中的那个数组元素。
- `Projection::apply_with_filter()`、`Filter::array_element_condition()` 公开 API。

## [0.6.0] - 2026-09-16

过滤与投影的语义补齐(第一档)。

### 新增
- **`$exists`**:判断字段是否存在。注意与 `$eq: null` **不等价** —— 字段缺失时两者都成立,字段值为 `null` 时 `$exists: false` 不成立。
- **`$type`**:14 种 BSON 类型别名;`"number"` 接受 int/long/double;对缺失字段不匹配任何类型。
- **`$all`**:数组须同时包含全部元素;`$all: []` 不匹配任何文档。
- **`$size`**:数组长度精确匹配;非数组不匹配。
- **投影 `$slice`**:`n`(负数从尾部取)、`[skip, limit]`。
- **投影 `$elemMatch`**:只保留第一个匹配元素。

## [0.5.4] - 2026-09-16

降序索引。

### 新增
- `IndexDef.directions`:与字段一一对应的索引方向(`1` 升序 / `-1` 降序)。
- `Database::create_index_with_directions()`。

### 修复
- **`sortedByIndex` 未比对方向导致返回顺序错**:降序索引被当成升序用,且计划层谎报「排序可由索引满足」从而跳过内存排序,静默返回错误顺序。现在方向不一致时保守返回 `false`。

## [0.5.3] - 2026-09-16

索引规划器扩展与 wire 层索引命令。

### 新增
- **`$or` 走索引**(`OR` 计划):各分支取并集;任一分支不可索引则退回全表扫。
- **多索引交集**(`AND` 计划):`{a:1, b:2}` 且各有索引时取候选集交集。
- wire 层 **`createIndexes` / `listIndexes` / `dropIndexes`** 真正落地(此前只回 `ok`,Go/Node 驱动无法建索引)。

### 修复
- `drop_collection` 未清理该集合的索引定义与条目。
- 计划构造的无限递归(`conjunction → intersection → …`)导致栈溢出。
- BSON 编码整数优先写 int32,导致游标 `id` 被驱动拒绝。

## [0.5.2] - 2026-09-16

复合索引与 wire 层 `explain`。

### 新增
- **复合索引**:遵循 MongoDB 前缀原则(`{a,b}` 可服务 `{a}`,反之不行);multikey 笛卡尔积展开(上限 128 组)。
- wire 层 **`explain`**:返回 `queryPlanner.winningPlan`。

## [0.5.1] - 2026-09-16

查询规划器 —— 索引真正被用上。

### 新增
- `plan_with_json`、`Plan`、`explain`;`Database::find_planned()`。
- 等值 / 范围 / 排序的索引区间构造。

## [0.5.0] - 2026-09-16

二级索引框架。

### 新增
- **order-preserving 编码**(`index/key.rs`):字节序 == BSON 值序,支撑 B+树 range 扫描。首字节复用 `query::compare::rank` 常量作为单一真相源。
- 独立索引表 `nosqlite_index_meta` / `nosqlite_index_data`。
- 6 个写路径同事务维护索引。

## [0.4.1] - 2026-09-16

### 修复
- 类型序修正为 MongoDB 官方 BSON 序:`MinKey < Null < Number < String < Object < Array < Binary < ObjectId < Bool < Date < Timestamp < Regex < MaxKey`。
- 补 `MinKey` / `MaxKey` / `Timestamp` 类型。

## [0.4.0] - 2026-09-16

### 新增
- `ScanSession` 真流式扫描(自持 `redb::ReadTransaction`),MVCC 快照隔离。
- `Database::scan()`。

## [0.3.1] - 2026-09-16

### 新增
- 真实游标:`getMore` / `killCursors`、`batchSize` 分批。

## [0.3.0] - 2026-09-16

### 新增
- **`nodblite-server`**:MongoDB wire 协议服务端。BSON 编解码零新依赖,OP_MSG / OP_QUERY,18 个命令。
- Go 官方驱动验收通过。

## [0.2.2] - 2026-09-16

### 新增
- `open_ex`(`struct_size` 版本化);`release.yml` 产出三平台库包 + SHA256。

## [0.2.1] - 2026-09-16

### 新增
- **C ABI**(`src/ffi.rs` + `include/nodblite.h`):不透明句柄、arena 结果、结构化错误、游标;`tests/c_smoke.c` 真实 C 程序冒烟;`examples/python/nodblite.py`。

## [0.2.0] - 2026-09-16

### 新增
- 阶段 0 契约冻结:`preserve_order`(字段序)、`ErrorCode`(12 码)、Extended JSON(`$oid`/`$numberLong`/`$numberDouble`)。

## [0.1.5] - 2026-09-16

### 新增
- CLI `.export` / `.import` / `.stats`。

## [0.1.4] - 2026-09-16

### 新增
- 8 个更新操作符(`$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`)与 upsert。

## [0.1.3] - 2026-09-16

### 新增
- Date 类型 `{"$date": ...}`,自实现 ISO-8601 解析,时区归一。

## [0.1.2] - 2026-09-16

### 修复
- 6 项查询语义对齐 MongoDB:同层 AND、`$in`/`$eq` 展开数组、点号穿透数组、缺失匹配 null、字段级 `$not`。

## [0.1.1] - 2026-09-16

### 新增
- `update_many` / `delete_many` 单事务(1000 条 7.69s → 24.75ms)。
- `Projection`、`find_page_total`、array/object 递归比较。

## [0.1.0] - 2026-09-16

首次发布。M1–M11:

- 单文件 `.nodb`、魔数 + 版本校验、后缀强制;
- 文档模型 + 主键索引 + 自动 ObjectId;
- `find` + 过滤子集 + 排序 / 分页;
- update / delete / count / collections / drop;
- sqlite3 风格 REPL(`.open`/`.collect`/`.databases` 等点命令);
- `$elemMatch` / `$unset`;
- 内存优先改造:流式化、`find_page` 两遍法、主键寻址、`insert_many` 单事务。
