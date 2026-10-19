# Changelog

版本号采用语义版本。完整设计与决策记录见 [AGENTS.md](./AGENTS.md) 的 `taolun` 章节。

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
- **`nosqlite-server`**:MongoDB wire 协议服务端。BSON 编解码零新依赖,OP_MSG / OP_QUERY,18 个命令。
- Go 官方驱动验收通过。

## [0.2.2] - 2026-09-16

### 新增
- `open_ex`(`struct_size` 版本化);`release.yml` 产出三平台库包 + SHA256。

## [0.2.1] - 2026-09-16

### 新增
- **C ABI**(`src/ffi.rs` + `include/nosqlite.h`):不透明句柄、arena 结果、结构化错误、游标;`tests/c_smoke.c` 真实 C 程序冒烟;`examples/python/nosqlite.py`。

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
