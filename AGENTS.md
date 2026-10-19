# AGENTS.md — nosqlite 项目 Agent 规范

> nosqlite:一个像 SQLite 一样小巧的单文件 NoSQL 数据库(文件后缀 `.nodb`),支持 Rust 内嵌 API 与 CLI。

## changelog

> 更新记录,按时间倒序(最新在上),条目需外链到对应 taolun 记录与项目进度。

- 2026.09.16: v0.1.0 发布 —— GitHub Release 附三平台 CLI 二进制(Linux x86_64 / macOS aarch64 / Windows x86_64 + SHA256,`release.yml` 自动构建);Cargo.toml 补齐 crates.io 元数据(rust-version 1.85/keywords/repository/license);README(定位/何时不用/跨平台性能表/内存与持久化语义)+ Apache-2.0 LICENSE;**crates.io 待发布**(需维护者 `cargo login`,`cargo publish --dry-run` 已验证通过)。

- 2026.09.16: v0.6.1 M29 完成 —— 数组更新 `$pull`/`$pop`(含对象子集匹配)+ 投影位置运算符 `{"arr.$": 1}`;**517 例全绿**。参见 [taolun #63](#63-v061-m29-设计数组更新-pullpop--投影位置运算符) / [taolun #64](#64-v061-m29-执行数组更新-pullpop--投影位置运算符) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.6.0 M28 完成 —— 第一档语义补齐:`$exists`/`$type`/`$all`/`$size` + 投影运算符 `$slice`/`$elemMatch`;**493 例全绿**。参见 [taolun #61](#61-v060-m28-设计第一档语义补齐existsallsize--投影运算符) / [taolun #62](#62-v060-m28-执行第一档语义补齐existsallsize--投影运算符) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.5.4 M27 完成 —— 降序索引(`directions` 随定义持久化,条目编码不变、仅扫描方向相反);修正 `sortedByIndex` 未比对方向导致**返回顺序错**的缺口;**466 例全绿**。参见 [taolun #59](#59-v054-m27-设计降序索引修正--1-被静默当升序) / [taolun #60](#60-v054-m27-执行降序索引) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.5.3 M26 完成 —— `$or` 走索引(并集)+ 多索引交集(AND 计划)+ wire 层 `createIndexes`/`listIndexes`/`dropIndexes` 真正落地(Go 驱动可建索引);`drop_collection` 补清索引;修掉计划构造的无限递归与复合索引被交集抢占两个缺陷;**457 例全绿**。参见 [taolun #57](#57-v053-m26-设计or-走索引--索引交集计划--wire-createindexes) / [taolun #58](#58-v053-m26-执行or-走索引--索引交集--wire-createindexes) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.5.2 M25 完成 —— 复合索引(前缀原则 + multikey 笛卡尔积)+ wire 层 `explain`;条目 key 全面改为「编码值在前、长度后缀在后」;**449 例全绿**。参见 [taolun #55](#55-v052-m25-设计复合索引--wire-层-explain) / [taolun #56](#56-v052-m25-执行复合索引--wire-层-explain) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.5.1 M24 完成 —— 查询规划器:等值/范围自动选索引、`explain` 可观测;**修掉 M23b 遗留的索引 key 长度前缀破坏值序缺陷**(导致跨类型范围查询漏数据)+ 写事务内开读事务缺陷;+12 例共 **440 例全绿**。参见 [taolun #53](#53-v051-m24-设计查询规划器让索引真正被用上) / [taolun #54](#54-v051-m24-执行查询规划器) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.5.0 M23b 完成 —— 索引框架:order-preserving 编码(复用 `rank` 单一真相源)、独立索引表、**multikey/稀疏/唯一**、写路径同事务维护;**核心验收「编码序 == 比较序」两两穷举通过**(并抓到 `$oid` 分叉);+35 例共 **427 例全绿**。参见 [taolun #51](#51-v050-m23b-设计索引框架order-preserving-编码--独立索引表) / [taolun #52](#52-v050-m23b-执行索引框架编码--独立索引表--写路径维护) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.4.1 M23a 完成 —— 类型序修正为 **MongoDB 官方 BSON 序**(MinKey<Null<Number<String<Object<Array<Binary<ObjectId<Bool<Date<Timestamp<Regex<MaxKey),补 MinKey/MaxKey/Timestamp;**破坏性变更**(跨类型比较);+10 例共 **392 例全绿**。参见 [taolun #49](#49-v041-m23a-设计类型序修正为-bson-官方序破坏性) / [taolun #50](#50-v041-m23a-执行类型序修正为-bson-官方序) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.4.0 M22 完成 —— `ScanSession` 真流式(会话自持读事务,`next_batch` 续取);`for_each` 改为其封装;+10 例共 **382 例全绿**。同时确立定位:**部署形态学 SQLite,数据模型/索引/游标语义 100% 学 MongoDB**。参见 [taolun #47](#47-v040-m22-设计scansession活游标--索引总方案) / [taolun #48](#48-v040-m22-执行scansession活游标) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.3.1 M21 完成 —— 真实游标:`getMore`/`killCursors` 落地,`batchSize` 分批生效,`limit` 语义修正(只约束总结果数);`aggregate` 同步接入游标;Go 驱动 13 项全通过,总 **372 例全绿**。参见 [taolun #45](#45-v031-m21-设计真实游标getmore--killcursors) / [taolun #46](#46-v031-m21-执行真实游标getmore--killcursors) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.3.0 M20 完成(阶段 2) —— **nosqlite-server 落地**:MongoDB wire 协议(OP_MSG + BSON 编解码 + 命令分发),**Go 官方驱动 9 项验收全通过**;367 例全绿。踩坑三条(section 1 / cursor.id 必须 int64 / OP_REPLY 长度)已写入认知修正。参见 [taolun #43](#43-v030-m20-设计阶段-2--nosqlite-servermongodb-wire-协议) / [taolun #44](#44-v030-m20-执行阶段-2--nosqlite-servermongodb-wire-协议) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.2.2 M19 完成(阶段 1.5) —— C ABI 收尾:新增 `open_ex`(兑现 `struct_size` 版本化)、`release.yml` 产出 `nosqlite-ffi-<platform>` 库包(动态库+静态库+头文件+SHA256)、README 补「获取库」「各语言接入」;**354 例全绿**。参见 [taolun #41](#41-v022-m19-设计阶段-15--c-abi-收尾可用性而非功能) / [taolun #42](#42-v022-m19-执行阶段-15--c-abi-收尾) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.2.1 M18 完成(阶段 1) —— **C ABI 落地**:`src/ffi.rs` + `include/nosqlite.h`,产出 cdylib/staticlib;骨架照搬 SQLite(不透明句柄 + 错误码 + 游标),现代化三点(arena 结果 / 结构化错误 / 版本化 options);真实 C 程序冒烟 + Python ctypes 实证;**354 例全绿**。参见 [taolun #39](#39-v021-m18-设计阶段-1--nosqlite-ffic-abi) / [taolun #40](#40-v021-m18-执行阶段-1--nosqlite-ffic-abi) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.2.0 M17 完成(阶段 0) —— 跨语言契约冻结:字段序保持(`preserve_order`,实测修复静默重排)、稳定错误码 `ErrorCode`(12 码,C ABI 整数 + wire 字符串)、Extended JSON 扩展(`$oid`/`$numberLong`/`$numberDouble`);+16 例共 **351 例全绿**。同时写入「双形态路线」总纲与「尽可能完整支持最新 MongoDB」的长期目标。参见 [taolun #37](#37-v020-m17-设计阶段-0--跨语言契约冻结双形态前置) / [taolun #38](#38-v020-m17-执行阶段-0-契约冻结) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.1.5 M16 完成 —— CLI `.export`(JSON Lines,分批写出)/`.import`(JSONL 或 JSON 数组,单事务整批)/`.stats`(文件大小 + 各集合条数);+10 例共 **335 例全绿**;顺带修正 `.help` 里残留的旧命令名。参见 [taolun #35](#35-v015-m16-设计cli-export--import--stats) / [taolun #36](#36-v015-m16-执行cli-export--import--stats) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.1.4 M15 完成 —— 更新操作符 `$inc/$mul/$min/$max/$rename/$push/$addToSet/$currentDate`(数组支持 `$each/$position/$slice/$sort`)+ upsert(`update_or_insert`);+50 例共 **325 例全绿**。参见 [taolun #33](#33-v014-m15-设计更新操作符扩展--upsert) / [taolun #34](#34-v014-m15-执行更新操作符扩展--upsert) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.1.3 M14 完成 —— Date 类型:`{"$date": "ISO-8601"}` / 毫秒时间戳,按时间戳比较排序、时区偏移归一,存储仍是普通 JSON(`.nodb` 格式 v1 不变);+17 例共 **275 例全绿**。参见 [taolun #31](#31-v013-m14-设计date-类型优先级-2) / [taolun #32](#32-v013-m14-执行date-类型) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.1.2 M13 完成 —— 查询语义与 MongoDB 对齐 6 项:同层条件改 AND、数组字段的 `$eq`/`$in` 按元素展开、点号路径穿透数组、缺失字段匹配 `null`、字段级 `$not`;+23 例共 **258 例全绿**。**含 4 项破坏性变更**,详见 README「与 MongoDB 的差异」。参见 [taolun #29](#29-v012-m13-设计查询语义与-mongodb-对齐优先级-1) / [taolun #30](#30-v012-m13-执行查询语义与-mongodb-对齐) / [项目进度-已完成](#已完成)。
- 2026.09.16: v0.1.1 M12 完成(A 组) —— 批量 `update`/`delete` 单事务化(1000 条 7.69 s → **24.75 ms,310.7x**)、字段投影 `--fields`、`find_page_total()` 一次扫描给「总数+当页」、array/object 递归比较(可排序、`$eq` 精确);+41 例共 **235 例全绿**。参见 [taolun #27](#27-v011-m12-设计补齐能上-cms-的最小闭环a-组) / [taolun #28](#28-v011-m12-执行a-组四项落地批量单事务--projection--一次扫描--复合比较) / [项目进度-已完成](#已完成)。
- 2026.09.16: M10 完成 —— `$elemMatch`(操作符/文档双形态)+ `$unset`(对象删除/数组置 null/路径缺失无操作);+15 例测试共 **194 例全绿**;顺带在 CLI 暴露 M11 能力(find --sort 走 find_page、insert 数组批量)。参见 [taolun #26](#26-20260916-m10-执行elemmatch--unset) / [项目进度-已完成](#已完成)。
- 2026.09.16: M11 完成 —— 内存优先改造：`find/count/update/delete` 流式化（内存从 O(集合) 降到 O(命中数)/O(1)）；新增 `find_page` 两遍法（500k 档 RSS 819 MiB → **30.2 MiB**，27x）、`get/update/delete_by_id` 主键寻址（2.6 s → **20.7 ms**，Linux 0.82 ms）、`insert_many` 单事务（**4.4 万条/秒** macOS、29.9 万 Linux）；+28 例测试共 179 例全绿；首次建立跨平台基准（macOS 被 `F_FULLFSYNC` 放大 **66x**）与 GitHub Actions CI。参见 [taolun #25](#25-20260916-m11-执行流式化--主键寻址--跨平台基准) / [项目进度-已完成](#已完成)。
- 2026.09.16: M9 补测完成 —— 写路径对照基准定位瓶颈为 **fsync 物理成本**(A/D = 1.08x @1k、0.76x @10k),而非引擎缺陷;批量单事务收益 **314x～1562x**;读路径补齐 10万/50万档实测,确认内存放大稳定在 **5.9–12.0 倍**(50万行单次 find 的 RSS 增量达 819 MiB)。参见 [taolun #23](#23-20260916-m9-补测写路径对照基准与归因) / [项目进度-已完成](#已完成)。
- 2026.09.16: M9 完成 —— 缺陷修复(#1 数组下标路径用对象覆盖数组的数据破坏缺陷;#2 空更新静默通过且虚增修改条数)+ 压测基线(实测写入吞吐仅约百条/秒、全量 find 的 RSS 增量达文档总字节的 5–12 倍)。参见 [taolun #21](#21-20260916-m9-设计缺陷修复--压测基线) / [taolun #22](#22-20260916-m9-执行) / [项目进度-已完成](#已完成)。
- 2026.08.12: M8 完成 —— REPL 对齐 sqlite3 点命令:管理命令一律点号开头(`.open/.databases/.collect/.help/.quit/.exit`),CRUD 一律无点,移除全部无点管理别名,`.collect` 替代 `.tables` 语义。参见 [taolun #20](#20-20260812-m8-执行) / [项目进度-已完成](#已完成)。
- 2026.08.12: M7 完成 —— CLI 默认 TTY(对齐 sqlite3):`nosqlite`/`nosqlite <file>` 无参或带路径直接进 REPL,文件不存在自动创建;移除单命令模式;REPL 内 `open/create/databases` 管理绑定。参见 [taolun #17](#17-20260812-m7-设计cli-默认-tty对齐-sqlite3) / [项目进度-已完成](#已完成)。
- 2026.08.12: M6 完成 —— CLI 交互式 shell(`nosqlite shell <file.nodb>` REPL,提示符 `nosqlite> `,shell 内命令不带文件路径)。参见 [taolun #15](#15-20260812-m6-设计cli-交互式-repl) / [项目进度-已完成](#已完成)。
- 2026.08.12: M5 完成 —— CLI 全命令(count/collections/drop)+ 单文件复制粘贴验证(tests/migration.rs 自动化回归)+ 测试补全(tests/manage.rs,112 例全绿)。参见 [taolun #13](#13-20260812-m5-设计) / [项目进度-已完成](#已完成)。
- 2026.08.12: M4 完成 —— update/delete,CRUD 闭环。`$set` 更新操作符(点号路径赋值,嵌套自动创建,`_id` 主键不可改),`Storage` trait 扩展 `update/delete`,CLI 新增 `update`/`delete` 命令。参见 [taolun #10](#10-20260812-m4-设计) / [项目进度-已完成](#已完成)。
- 2026.08.12: M3 完成 —— find + filter 子集($eq/$gt/$in/$regex 等)+ sort/limit/skip,BSON 风格类型序,点号路径与逻辑组合。参见 [taolun #7](#7-20260812-m3-设计) / [项目进度-已完成](#已完成)。
- 2026.08.12: M2 完成 —— 文档模型 + insert + 主键索引(自动/显式 `_id`,重复主键拒绝,集合隔离)。参见 [taolun #4](#4-20260812-m2-设计) / [项目进度-已完成](#已完成)。
- 2026.08.12: M1 完成 —— 项目骨架 + AGENTS.md + `.nodb` 文件创建/打开(魔数头 + 版本校验,拒绝非法后缀)。参见 [taolun #2](#2-20260812-m1-执行) / [项目进度-已完成](#已完成)。

## taolun

> 对话记录,按时间正序(最早在上)。每次对话先追加到这里,再动手改代码。

### 1. 2026.08.12 需求讨论与总体方案

- 目标:做一个"像 SQLite 一样"的单文件 NoSQL 数据库,文件后缀固定为 `.nodb`,任何平台不允许其他后缀或无后缀。
- 三种访问形态:CLI 直接操作、Rust 内嵌 API(`open/exec/find`)、TS/Go 通过 MongoDB driver 访问。
- 架构:存储保持嵌入式单文件(可复制粘贴),外层套 HTTP/协议服务(后期)。MVP 先做核心存储 + CLI。
- 聚合管道场景占比低,决定 MVP 不做,先做使用场景多的 find + CRUD。
- 选型决定:
  - 存储引擎:先现成后自研 —— 先用 redb(纯 Rust 单文件 B+树),通过 `Storage` trait 抽象,后期可替换自研。
  - OOP 落地:Rust 惯用方式(`struct + impl + trait`),模块高内聚低耦合。
  - 语言:Rust(lib + bin 同 crate,单二进制)。

### 2. 2026.08.12 M1 执行

- 创建 AGENTS.md,建立 changelog / taolun / Agents / 认知修正 四段式结构。
- 初始化 Cargo 项目骨架(lib + bin,单二进制)。Cargo.toml 引入 redb v3.1.3 作为底层单文件 B+树引擎,经 `Storage` trait 抽象(先现成后自研)。
- 模块结构:`src/lib.rs`(对外 API 入口)、`src/engine/{mod,storage,redb_engine}.rs`(引擎 + trait + redb 实现)、`src/cli.rs`(类 sqlite3 CLI,薄壳 `main.rs` 调用)。
- 实现 M1:`.nodb` 创建/打开。后缀强制校验(不区分大小写,其他后缀/无后缀一律 `InvalidPath` 拒绝);魔数串 `NOSQLITE` + 格式版本 v1 存于 redb 元数据表 `nosqlite_meta`,打开时校验魔数与版本,版本不兼容给出升降级提示。
- CLI 子命令:`create`(目标文件已存在则拒绝防误覆盖)、`open`、`help`,全部中文输出。
- 编写按功能模块拆分的集成测试 `tests/engine.rs`,9 个用例全绿:创建/打开往返、防重复创建、各种非法后缀、无后缀、大写后缀、打开不存在/垃圾文件/纯 redb 文件、非法后缀打开。
- CLI 冒烟验证:非法 `.db` 拒绝、创建/打开成功、复制粘贴 `.nodb` 后仍可打开(单文件可迁移成立)。

### 3. 2026.08.12 发布方式确认

- 版本号约定 `YYYY.MM.DD.xxxx`,`xxxx` 为打 git tag 前的 commitID,便于溯源。
- 本里程碑 M1 先由当前 commit 打 tag 发布,后续每个里程碑同样处理。

### 4. 2026.08.12 M2 设计

- M1 打 tag `2026.08.12.df632a5` 发布。
- M2 范围:文档模型 + insert + 主键索引。
- 文档模型设计:
  - 文档顶层为 JSON 对象,用 `serde_json::Map<String, Value>` 包装成 `Document`(支持嵌套任意 Value)。
  - 主键 `_id`:用户可显式提供;未提供时自动生成 12 字节 ObjectId 风格 ID(4 字节时间戳 + 5 字节随机 + 3 字节计数器,hex 编码 24 字符),与 MongoDB 对齐。
  - 序列化:文档 JSON bytes 作为 value。
- 存储布局:单表 `nosqlite_docs`,key = `集合名(0x00)doc_id` 字节拼接,value = 文档 JSON bytes。redb B+树天然按 key 有序,集合内 doc_id 有序即主键索引。
- 重复 `_id` 插入 → 报"主键冲突"错误。
- 文件格式版本保持 v1:旧 M1 空库可被 M2 直接打开使用(写事务中自动建 docs 表),无需升降级。

### 5. 2026.08.12 M2 执行

- 新增 `document` 模块:`Document`(顶层 JSON 对象,支持嵌套任意 Value,`from_json/to_bytes/ensure_id` 等)+ `ObjectId` 生成器(12 字节 = 4 字节时间戳 + 5 字节伪随机 + 3 字节计数器,hex 编码 24 字符,与 MongoDB 对齐;伪随机种子 = 启动纳秒 ^ PID)。
- 新增 `database` 门面:`Database::create/open/insert/close`,`insert` 自动生成或沿用 `_id` 并回填,非字符串 `_id` 报 `IdNotString`。
- `Storage` trait 扩展 `insert(collection, doc_id, doc_bytes)`,redb 实现采用单表 `nosqlite_docs`,key = `集合名(0x00)doc_id` 字节拼接(集合名校验:非空、不含 `\0`),value = 文档 JSON bytes;redb B+树按 key 有序,集合内 doc_id 有序即主键索引。
- 主键冲突检测:利用 redb `insert` 返回旧值判断重复,报 `DuplicateKey` 错误。
- CLI 新增 `insert <file> <collection> <json>` 命令,全中文输出。
- 文件格式版本保持 v1:旧 M1 空库可直接打开使用(写事务自动建 docs 表),无需升降级。
- 按功能模块拆分测试:`tests/document.rs`(10 例)、`tests/insert.rs`(6 例),连同 `tests/engine.rs`(9 例)与 ObjectId 单测(3 例)共 28 例全绿。
- CLI 冒烟验证:自动/显式 ID、重复主键拒绝、非对象顶层拒绝、复制粘贴 `.nodb` 后数据仍可打开。

### 6. 2026.08.12 M2 发布

- M2 打 tag `2026.08.12.<commitID>` 发布。

### 7. 2026.08.12 M3 设计

- M2 打 tag `2026.08.12.1da8f95` 发布。
- M3 范围:find + filter 子集 + sort/limit/skip。
- 查询语法设计:仿 MongoDB 文档过滤。
  - filter 顶层为 JSON 对象;每个字段的值若是普通 Value 则视为隐式 `$eq`;
  - 值若是对象且键以 `$` 开头,则视为操作符对象,同一字段多个操作符按 AND 组合;
  - 操作符子集:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`;支持点号路径(如 `a.b.c`);
  - 顶层支持 `$and/$or/$nor/$not` 逻辑组合;
  - 类型序:MongoDB BSON 风格,缺失 < null < bool < number < string < array < object;
  - number 内部按数值比较(int/float 可互比);`$regex` 用 Rust `regex` crate 匹配字符串字段。
- sort/limit/skip:sort 为 `{"age": 1, "name": -1}` 形式的字段方向映射(1 升序 / -1 降序),支持多字段与点号路径;limit/skip 应用于排序后的结果。
- 存储侧:`Storage` trait 扩展 `scan(collection)` 返回集合内所有文档(id + bytes);find 在门面层反序列化 → 过滤 → 排序 → 分页。全量扫描对 MVP 规模足够,M3 不做索引下推。

### 8. 2026.08.12 M3 执行

- 新增 `query` 模块:`compare`(BSON 风格类型序比较:缺失 < null < bool < number < string < array < object,number 按数值互比)、`filter`(操作符子集 `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`,点号路径,顶层 `$and/$or/$nor/$not`,普通值视为隐式 `$eq`,同字段多操作符按 AND 组合)、`options`(`SortSpec` 多字段排序 `{"age":1,"name":-1}` + `FindOptions` skip/limit)。
- `Storage` trait 扩展 `scan(collection)`,redb 实现按 key 前缀(`集合名(0x00)`)遍历,返回 `(doc_id, doc_bytes)`;文档表不存在(空库)时返回空列表而非报错。
- `Database::find` / `find_with_options` 端到端流水线:扫描 → 反序列化 → 过滤 → 排序 → skip → limit。
- CLI 新增 `find <file> <collection> [filter] [--sort <json>] [--skip <n>] [--limit <n>]` 命令,全中文输出。
- 按功能模块拆分测试:`tests/query.rs`(30 例)、`tests/find.rs`(13 例),连同现有测试共 81 例全绿。
- CLI 冒烟验证:空 filter 全量、`$gte`、`$regex`、sort+limit 降序取前 2 均正确。

### 9. 2026.08.12 M3 发布

- M3 打 tag `2026.08.12.<commitID>` 发布。

### 10. 2026.08.12 M4 设计

- M3 打 tag `2026.08.12.4efebdc` 发布。
- M4 范围:update/delete,CRUD 闭环。
- 更新语义设计(仿 MongoDB):
  - filter 匹配与 M3 find 完全一致(操作符子集 + 点号路径 + 逻辑组合);
  - `update(collection, filter, update)` 对所有匹配文档执行更新,返回修改条数;
  - 更新操作符先做 `$set`(字段点号路径赋值,嵌套自动创建/覆盖);
  - `delete(collection, filter)` 删除所有匹配文档,返回删除条数;
  - 更新后 `_id` 保持不变(只改内容,不动主键)。
- 存储侧:`Storage` trait 扩展 `update(collection, doc_id, doc_bytes)` 与 `delete(collection, doc_id)`:
  - redb `update` 直接覆盖 key 对应 value,主键不变;
  - redb `delete` 删除 key 对应条目;
  - 二者均在写事务中完成,返回受影响条数。
- 门面层流水线:扫描 → 过滤 → 对匹配文档执行更新/删除 → 逐个写回/移除。
- 说明:由于存储以 doc_id 为主键索引,update 仅支持"替换字段"而非"改主键";M4 不做索引下推。

### 11. 2026.08.12 M4 执行

- 新增 `query::update` 模块:`Update`(仅 `$set`,点号路径赋值,中间对象自动创建/覆盖,`_id` 主键不可修改报错,未知操作符报错)与 `UpdateError`。
- `Storage` trait 扩展 `update(collection, doc_id, doc_bytes)`(覆盖写,主键不变,返回是否实际更新)与 `delete(collection, doc_id)`(删除 key,返回是否实际删除);redb 实现均在写事务中完成,复用 `insert` 覆盖语义与 `remove`。
- `Database::update(collection, filter, update)` 与 `Database::delete(collection, filter)`:扫描 → 过滤 → 应用更新/删除 → 逐个写回/移除,返回修改/删除条数;`DatabaseError` 新增 `Update(UpdateError)` 变体。
- CLI 新增 `update <file> <collection> <filter> <update>` 与 `delete <file> <collection> <filter>` 命令,全中文输出;帮助信息同步更新。
- 按功能模块拆分测试:`tests/update.rs`(9 例)、`tests/delete.rs`(9 例),连同现有测试共 99 例全绿;`cargo build/clippy --all-targets` 零警告。
- CLI 冒烟验证:`$set` 更新 2 条、`delete` 条件删除、复制粘贴 `.nodb` 后数据仍完整(单文件可迁移成立)。

### 12. 2026.08.12 M4 发布

- M4 打 tag `2026.08.12.663e4c9` 发布。

### 13. 2026.08.12 M5 设计

- M4 打 tag `2026.08.12.663e4c9` 发布。
- M5 范围:CLI 全命令 + 单文件复制粘贴验证 + 测试补全。
- CLI 命令补全(对齐 CRUD + 管理场景):
  - `count <file> <collection> [filter]`:统计满足 filter 的文档条数(filter 缺省统计全集合);
  - `collections <file>`:列出数据库内全部集合名;
  - `drop <file> <collection>`:删除整个集合及其全部文档(不可恢复);
  - 保留 create/open/insert/find/update/delete/help。
- 存储侧:`Storage` trait 扩展:
  - `list_collections()`:遍历文档表 key(有序),按 `集合名(0x00)` 前缀提取去重后的全部集合名;
  - `drop_collection(collection)`:遍历该集合前缀删除全部 key,返回删除条数(集合不存在返回 0 不报错);
  - 二者均在事务中完成。
- 门面层:`Database::count/find` 计数复用 find 过滤语义;`Database::collections()`、`Database::drop_collection()` 代理引擎。
- 单文件复制粘贴验证:新增 `tests/migration.rs` 自动化测试 —— 创建 → 插入多集合多文档 → 关闭 → 复制 `.nodb` 副本 → 打开副本校验数据与 _id 完整一致(把 CLI 冒烟验证固化为自动化回归)。
- 测试补全:新增 `tests/manage.rs`(count/collections/drop 门面 + CLI 行为);M5 收尾共 ~120 例全绿。

### 14. 2026.08.12 M5 执行

- CLI 命令补全:`count <file> <collection> [filter]`(filter 缺省统计全集合)、`collections <file>`(列出全部集合,空库提示"暂无集合")、`drop <file> <collection>`(删除整个集合,返回删除条数,不可恢复);与既有 create/open/insert/find/update/delete/help 组成完整命令集,全中文输出,帮助信息同步更新。
- `Storage` trait 扩展 `list_collections()` 与 `drop_collection(collection)`:redb 实现利用 key = `集合名(0x00)doc_id` 的有序性,遍历提取去重集合名(集合名必含分隔符,相同集合 key 相邻故去重只需比较上一个);drop 写事务内先收集该前缀全部 key 再逐个删除,返回删除条数,集合不存在返回 0 不报错。
- `Database::count/collections/drop_collection` 门面:`count` 复用 scan + filter 过滤语义(与 `find(...).len()` 等价),`collections`/`drop_collection` 代理引擎。
- 单文件复制粘贴验证:新增 `tests/migration.rs`(2 例)—— 复制 `.nodb` 副本后打开,多集合多文档数据与 _id 完整一致;副本可独立 update/delete 且不影响原库(把 CLI 冒烟验证固化为自动化回归)。
- 测试补全:新增 `tests/manage.rs`(11 例:count 全量/带 filter/空集合/与 find 一致,collections 去重有序/空库,drop 删除/不存在返回 0/再插入/持久化),连同现有测试共 112 例全绿;`cargo build/clippy --all-targets` 零警告。
- CLI 冒烟验证:count 全量与条件统计、collections 列出两集合、drop 后集合消失且其余集合不受影响均正确。

### 15. 2026.08.12 M6 设计:CLI 交互式 REPL

- M5 打 tag `2026.08.12.d1633f8` 发布。
- 需求:当前 CLI 为单命令模式(`nosqlite <命令> <file> ...`),每次都要带文件路径;用户希望像 sqlite3 一样进入 TTY 交互式 shell。
- 设计:`nosqlite shell <file.nodb>` 进入交互模式(新子命令,保留单命令模式不变):
  - 打开数据库后进入 REPL,提示符 `nosqlite> `;
  - shell 内命令不带文件路径参数,集合命令与单命令对齐:`insert/find/update/delete/count/drop <collection> ...`、`collections`(无参);
  - `help` 显示 shell 内命令用法;`quit`/`exit` 退出 shell;
  - 空行忽略;每行一条命令;命令解析复用现有 filter/update 解析逻辑;
  - 命令执行错误打印后继续循环,不退出。
- 实现:`Command` 增加 `Shell(String)` 变体;`parse` 识别 `shell <file>`;新增 `run_shell(path)` 循环读取 stdin 每行,行内解析为「shell 命令」再分发到既有 run_* 函数(抽出集合命令为内部函数,避免重复)。
- 测试:新增 `tests/cli_shell.rs` 交互命令解析(复用 `parse` 测试单命令);交互 stdin 不易自动化,以解析层测试 + CLI 冒烟验证为主。

### 16. 2026.08.12 M6 执行

- `Command` 增加 `Shell(String)` 变体,`parse` 识别 `shell <file>`;新增 `run_shell(path)`:打开数据库 → 打印欢迎语 → 循环读取 stdin 每行 → `parse_shell` 解析为 `ShellCommand`(无路径)→ `exec_shell` 分发执行 → 空行忽略,错误打印后继续循环,`quit/exit/.quit/.exit` 退出,EOF(Ctrl+D)退出。
- 集合命令抽出为「已打开数据库」内部函数 `exec_insert/exec_find/exec_update/exec_delete/exec_count/exec_collections/exec_drop`,单命令模式的 `run_*` 变为 `open_database(path)` + `exec_*` 的薄壳,避免 shell 与单命令两套逻辑重复。
- shell 内命令与单命令对齐:`insert/find/update/delete/count/drop <collection> ...`、`collections`(无参);find 支持 `--sort/--skip/--limit`;`help/?` 显示 shell 用法,`quit/exit` 退出;提示符 `nosqlite> `。
- `parse_shell` 与 `ShellCommand` 公开导出,便于集成测试。
- 新增 `tests/cli_shell.rs`(11 例:quit/help 各形态、insert/find(含选项)/update/delete/count/collections/drop 解析、空行与缺参/未知命令报错),连同现有测试共 123 例全绿;`cargo build/clippy --all-targets` 零警告。
- CLI 冒烟验证:stdin 管道注入多行命令,insert/collections/count/find/update/delete/drop/help/quit 全部正确执行(注:PowerShell 双引号 here-string 会插值 `$set/$gte`,冒烟须用单引号 here-string)。

### 17. 2026.08.12 M7 设计:CLI 默认 TTY(对齐 sqlite3)

- M6 打 tag `2026.08.12.2fb977f` 发布。
- 认知修正:M6 采用 `nosqlite shell <file>` + 单命令并存,但 sqlite3 的 CLI 语义是**默认直接进入 TTY**,且**不依赖数据库文件是否存在**——不管有没有库都能进入,在 TTY 内创建/加载数据库。M6 的形态偏离目标。
- 设计(对齐 sqlite3):
  - `nosqlite`(无参数):直接进入 REPL,**未绑定数据库**状态;
  - `nosqlite <file.nodb>`:打开该库进入 REPL,文件不存在则**自动创建**(对齐 sqlite3 打开即建);
  - **移除单命令模式**(`create/open/insert/find/update/delete/count/collections/drop <file> ...` 子命令全部删除,只留 TTY;Rust 内嵌 API 与 Storage trait 不受影响);
  - REPL 内数据库管理命令:`open <file>`(关闭当前,不存在则创建/存在则打开)、`create <file>`(新建,已存在拒绝,沿用 M1 防误覆盖)、`databases`(显示当前绑定);
  - 集合命令在未绑定数据库时报「尚未打开数据库,请先执行 open/create」,不崩溃;
  - `run_shell` 持有 `Option<Database>` 可变状态,切换绑定前 drop 旧句柄(Windows redb 独占锁约束)。
- 实现:`cli.rs` 重写入口 `run()`(无参/单路径/帮助三种形态);`exec_shell` 改为接收 `&mut Option<Database>`;删除单命令 `Command/parse/execute/run_*`;保留 exec_* 内部函数与 `parse_shell`/`ShellCommand`(公开导出)。
- 测试:`tests/cli_shell.rs` 扩展 open/create/databases 解析;冒烟验证用 stdin 管道覆盖 create→open→databases→insert→quit 与自动创建进入。

### 18. 2026.08.12 M7 执行

- `cli.rs` 重写入口 `run()`:无参数 → `run_shell(None)`(未绑定数据库直接进 REPL);单路径 <file.nodb> → `open_or_create(path)`(先尝试打开,不存在则 `Database::create` 自动创建,对齐 sqlite3"打开即建")→ `run_shell(Some(db))`;`-h/--help/help` → 打印用法;参数过多 → 报错。
- 移除单命令模式:删除 `Command` 枚举、`parse`、`execute`、`run_insert/find/update/delete/count/collections/drop` 薄壳、`report`、`open_database`;保留 `exec_*` 内部函数与 `parse_shell`/`ShellCommand`(公开导出)。
- `run_shell(initial: Option<Database>)` 持有 `current: Option<Database>` 可变绑定状态,切换绑定前 `drop(current.take())` 释放旧句柄(Windows redb 独占锁约束);集合命令通过 `require_db` 门卫,未绑定时报「尚未打开数据库,请先执行 `open <file.nodb>` 或 `create <file.nodb>`」,不崩溃。
- `ShellCommand` 新增数据库管理命令:`open <file>`(关闭当前,不存在则创建/存在则打开)、`create <file>`(新建,目标已存在拒绝,沿用 M1 防误覆盖)、`databases`(别名 `db`,显示当前绑定);`exec_shell` 签名改为 `(&mut Option<Database>, cmd)`。
- `tests/cli_shell.rs` 扩展 4 例(open/create/databases 解析 + open/create 缺路径报错),连同现有测试共 127 例全绿;`cargo build/clippy --all-targets` 零警告。
- CLI 冒烟验证(单引号 here-string 注入 stdin):无参进入未绑定提示 + 未绑定执行 find 报错提示 + create/databases/insert/collections/count 正常;`nosqlite <不存在>.nodb` 自动创建后进入、再进入数据仍持久化;非法后缀 create 拒绝;`--help` 显示用法;参数过多报错。均正确。

### 19. 2026.08.12 M8 设计:REPL 对齐 sqlite3 点命令

- M7 打 tag `2026.08.12.583016c` 发布。
- 认知修正:M7 的 shell 是"无点命令 + 集合操作"混排,亲和度不足;sqlite3 REPL 是「点命令(管理)+ 无点语句(CRUD)」二元结构,应严格对齐。
- 设计(用户确认三点):
  - **严格点命令**:数据库管理命令一律点号开头,CRUD 一律无点;
  - **`.collect`** 列出全部集合(替代 sqlite3 `.tables` 语义,集合=collection);
  - **集合隐式创建**:insert 到不存在的集合即自动创建,不引入显式 create collection(对齐 MongoDB,无需存储层改动)。
- 命令映射:
  - `.open <file>`:打开数据库,不存在则自动创建;无参 = 关闭当前回到未绑定(无内存库,即提示如何 .open);
  - 移除 `create <file>`(冗余:`.open` 已存在就打开、不存在就建,天然防误覆盖);
  - `.databases`:显示当前绑定;
  - `.collect`:列出全部集合;
  - `.help` / `.quit` / `.exit`:点命令;
  - `insert/find/update/delete/count/drop <collection> ...`:保持无点(CRUD);
  - **移除全部无点管理别名**(open/create/databases/db/collections/quit/exit/help/?);
  - 提示符保持 `nosqlite> `。
- 实现:`cli.rs` 的 `ShellCommand` 删除 `Create`,`.open` 无参分支关闭绑定;`parse_shell` 重写点命令/CRUD 二分解析;`exec_open` 支持无参;`print_shell_help` 改为 sqlite3 `.help` 风格两段式。
- 测试:`tests/cli_shell.rs` 更新 `.open/.databases/.collect` 解析 + `.open` 无参 + 无点管理命令报错。
- 说明:MVP 阶段 `.collect` 不支持 pattern 通配(先无参列出全部);后续按需扩展。

### 20. 2026.08.12 M8 执行

- `ShellCommand` 重构:`Open(Option<String>)`(无参 = 关闭当前回到未绑定),新增 `Collect`,删除 `Create`/`Collections` 与全部无点管理别名(`open/create/databases/db/collections/help/?/quit/exit` 均报「管理命令必须用点号」提示)。
- `parse_shell` 重写为点命令/CRUD 二分解析;`exec_open` 支持无参;`print_shell_help` 改为 sqlite3 `.help` 风格;欢迎语同步为 `.help`/`.quit` 提示。
- 测试:`tests/cli_shell.rs` 更新 `.open` 带/无参、`.databases`、`.collect` 解析,新增无点管理别名拒绝与 `.quit/.exit` 用例,共 127 例全绿;`cargo build/clippy --all-targets` 零警告。
- 冒烟验证:无参进入未绑定提示、`.open` 打开/自动创建、无点 CRUD(insert/find/update/delete/count)与 `.collect/.databases` 全部正确;未绑定执行 find 提示先 `.open`;非法后缀拒绝;数据持久化(重新进入数据仍在)。修复:执行函数改名 `exec_collections` → `exec_collect`、欢迎语文案对齐点命令。

### 21. 2026.09.16 M9 设计:缺陷修复 + 压测基线

- 背景:通用 CMS 后台项目决定采用 Rust 后端 + 内嵌 nosqlite 作存储(不引入外部数据库,保持单二进制)。开工前先把 nosqlite 在该场景下暴露的缺陷与缺口补掉,避免 CMS 开发中反复回补。
- 核查方法:对参考项目 CloudBase CMS(`packages/service`)全量 grep 数据层调用,统计**实际用到的全部操作符与访问模式**,据此确定缺口清单(不凭猜测)。
- 实际用到的操作符与现状:
  - 已覆盖:`$in`(6 处)、`$or`、`$and`、`$gte`/`$lte`、`$eq`、`$set`(含点号路径)。
  - 缺失:`$elemMatch` —— 4 处,且**全部是「数组包含某标量」语义**(`role.controller.ts:109`、`contents.controller.ts:96`、`webhooks.service.ts:71`、`schema.controller.ts:65`)。
  - 缺失:`$push`/`$pull` —— 仅出现在 `setting.controller.ts`,分别服务 microApps(微搭专属,不迁)与 apiAuthTokens(可改应用层读-改-写)。
- 核查出的缺陷与缺口清单:

| 编号 | 问题 | 位置 | 性质 |
|---|---|---|---|
| #1 | 数组下标路径用对象覆盖数组(数据破坏) | `src/query/update.rs::set_by_path` | 缺陷(严重) |
| #2 | 空更新静默通过,且仍计入 modified 并写回 | `update.rs::from_value` + `database.rs::update` | 缺陷 |
| #3 | 不支持 `$elemMatch`/数组包含 | `src/query/filter.rs` | 功能 |
| #4 | 不支持 `$unset` | `update.rs::from_value`(白名单仅 `$set`) | 功能 |
| #5 | 分页双扫描(count 与 find 各一次全量) | `database.rs::count`/`find_with_options` | 效率 |
| #6 | `scan` 返回 `Vec<(String, Vec<u8>)>`,整集合入内存 | `src/engine/storage.rs::scan` | 架构风险 |
| #7 | 无 `insert_many`,逐条写事务 | `database.rs::insert` | 效率 |

- 决策:
  - 顺序:**先修缺陷 → 建压测基线 → 按数据补功能 → 按数据做效率改造**。理由:效率类改造的「怎么补」取决于压测数据,盲补会白做。
  - 明确不补(省时且保持通用):`$push`/`$pull`、聚合管道/`$lookup`/join、跨集合事务、唯一约束与索引(本阶段)、Server 模式。
  - **不升 `.nodb` 文件格式版本**:全部改动必须在 v1 内兼容,旧库可直接打开、复制粘贴迁移必须继续成立。
  - 纪律:nosqlite 内**不出现任何 CMS 专属概念**(`_collections`/`schema`/`permission` 等),保持「MongoDB 对齐的通用文档库」。
  - 数组包含方案:M10 采用**新增 `$elemMatch`**(语义与 MongoDB 一致),而非给隐式 `$eq` 加数组包含语义 —— 后者会改变既有比较语义,需全量回归。
- 里程碑规划:
  - M9:缺陷修复(#1、#2)+ 压测基线,**不含新功能**。
  - M10:过滤能力补齐(`$elemMatch`、`$unset`),既有 127 例测试必须全绿。
  - M11:效率改造(只做 M9 数据证明必要的项:`scan` 迭代器化、`find_page()` 单次扫描、`insert_many` 单事务)+ 收尾 + CLI 同步。
  - M12:验收与发布(`clippy --all-targets` 零警告、全测试绿、更新 changelog/进度、打 tag)。

### 22. 2026.09.16 M9 执行

- 缺陷 #1(数组下标路径):模块文档同步;`src/query/update.rs` 重写路径赋值 —— 拆为 `set_in_map`(顶层对象)+ `set_in_value`(任意 JSON 值,明确区分 `Value::Object`/`Value::Array`/标量)+ `parse_array_index`;数组按下标寻址,下标非数字报 `InvalidArrayIndex`,越界报 `ArrayIndexOutOfBounds { index, len }`,标量中间值报 `ScalarIntermediate` 而**不再用对象覆盖**。新增 `tests/update_array.rs`(10 例:下标赋值、元素嵌套字段、多位下标、元素新增字段、多级混合路径、越界含 `index == len` 边界、非法下标、标量中间值、持久化重开)。
- 缺陷 #2(空更新):`Update::from_value` 拒绝空对象、空 `$set`、空路径段(`a..b`/`.a`/`a.`);`Database::update` 对内容无变化的文档**不写回、不计数**,对齐 MongoDB `modifiedCount`。新增 `tests/update_validation.rs`(8 例)。
- 顺手修复:既有 `tests/cli_shell.rs` 中重复的 `#[test]` 属性(导致该用例被重复注册,也是 `clippy` 唯一告警源)。
- 压测基线:新增两个 release 专用示例(不进入测试套件)——`examples/bench.rs`(耗时/内存,默认 1万/10万/50万,可传参)、`examples/bench_concurrency.rs`(单线程 vs 多线程写入对比);新增 `tests/concurrency.rs`(6 例:`Database` 的 `Send + Sync` 编译期断言、同集合并发插入、多集合并发插入、竞争下主键冲突恰好一个成功、边写边读计数单调不减、多线程更新互不干扰)。
- 实测数据(macOS / release):

| 规模 | insert 全部 | find 全量 | count 全量 | count 条件 | 排序+深分页 | update 单条 | delete 单条 | scan 物化 | find 的 RSS 增量 |
|---|---|---|---|---|---|---|---|---|---|
| 1,000 | 23.1 s | 8.7 ms | — | — | — | — | — | 0.18 MiB | +2.0 MiB |
| 2,000 | 44.6 s | 12.2 ms | — | — | — | — | — | 0.35 MiB | +2.3 MiB |
| 4,000 | 116.7 s | 24.1 ms | — | — | — | — | — | 0.71 MiB | +3.7 MiB |
| 10,000 | 340.7 s | 66.4 ms | 50.8 ms | 42.5 ms | 106.1 ms | 104.7 ms | 69.8 ms | 1.77 MiB | +22.0 MiB |

- 并发写入(总量 160 条):单线程 1.726 s(92.7 条/秒) / 8 线程 1.151 s(139.0 条/秒),**加速比仅 1.50x** —— 写入被单写事务串行化,符合 B+树单写者模型。
- 数据得出的结论(直接决定 M11 范围):
  - **#7 写入是最大瓶颈**:逐条插入各起一个写事务、每次 commit 都要 fsync,实测 4–34 ms/条(约 **100 条/秒**量级),且随文件增长恶化。→ `insert_many` 单事务为**必须项**,预期提升 1–2 个数量级。
  - **#6 `scan` 内存锤实**:`find` 的 RSS 增量约为文档总字节的 **5–12 倍**。10k 文档(1.77 MiB)已达 +22 MiB;线性外推 100k/500k 的 payload 约 18/88 MiB → RSS 增量可达 **0.1–1 GiB**,且每次全量 `find` 都会产生这个瞬时峰值,并发叠加会 OOM。→ `scan` 迭代器化为**必须项**。
  - **#5 分页双扫描**:`find` 全量 10k = 66.4 ms(线性,~6.6 µs/条)→ 外推 100k ≈ 0.66 s、500k ≈ 3.3 s;而内容列表要 `count` + `find` 各一次即双倍。→ `find_page()` 共用一次扫描为**必须项**。
  - **额外发现(不在原清单内)**:`update`/`delete` 即便按 `_id` 也走全表 `scan`(10k 下单条 update 104.7 ms、delete 69.8 ms)。→ 需暴露**主键直接寻址**(B+树主键索引已存在,只是未开放 API)。
  - **10 万/50 万档未实测**:构建这两个数据集本身被写入瓶颈卡住(100k 逐条插入需数十分钟、500k 需数小时),耗时不可接受(已在 taolun #23 的补测中完成)。

### 23. 2026.09.16 M9 补测:写路径对照基准与归因

- 背景:taolun #22 的压测只给了绝对值,缺参照系,无法回答「写入慢是引擎缺陷还是 fsync 的物理必然」;且同一 1000 条逐条插入曾测出 4.19 s 与 23.1 s 两种结果,说明单次测量被机器负载污染,不足定性。
- 补测项:(a) 写路径四组对照 + 归因;(b) 受控重测(预热 + 多次取中位数 + 报极差);(c) 读路径补齐 10 万/50 万档。
- 新增 `examples/bench_write.rs`(写路径四组对照)、重写 `examples/bench.rs`(读路径,支持 `--repeat=N`、报中位数与极差)。
- **关键方法变更**:读基准的数据改用**批量单事务**直接写入 redb(沿用引擎的 key 布局与元数据表,生成标准 `.nodb`,再用 `Database::open` 打开测量)。原因:逐条插入的写瓶颈已单独量化,若用它准备数据则无法把规模做到 10 万/50 万。
- 写路径对照(文档结构与值完全一致,每组预热一次后重复 3 次取中位数):

| 条数 | 组 | 中位 | 极差 | 吞吐 |
|---|---|---|---|---|
| 1,000 | A nosqlite 逐条插入(每条一个写事务/Immediate) | 4.187 s | 4.161–4.217 s | 238.8 条/秒 |
| 1,000 | B redb 批量单事务(Immediate) | 13.35 ms | 12.82–19.22 ms | 74,905 条/秒 |
| 1,000 | C redb 批量单事务(None + 末尾落盘) | 19.05 ms | 14.87–19.52 ms | 52,498 条/秒 |
| 1,000 | D fsync 基准(追加 + `sync_all`) | 3.869 s | 3.858–3.899 s | 258.5 次/秒 |
| 10,000 | A nosqlite 逐条插入 | 228.900 s | 168.015–295.263 s | 43.7 条/秒 |
| 10,000 | B redb 批量单事务(Immediate) | 202.91 ms | 174.87–217.74 ms | 49,284 条/秒 |
| 10,000 | C redb 批量单事务(None + 末尾落盘) | 146.57 ms | 120.60–176.62 ms | 68,227 条/秒 |
| 10,000 | D fsync 基准 | 302.019 s | 283.613–304.464 s | 33.1 次/秒 |

- 归因结论:
  - **写入瓶颈就是 fsync 的物理成本,不是 nosqlite/redb 的实现缺陷**:A/D = **1.08x**(1,000 条)/ **0.76x**(10,000 条),即 nosqlite 每条插入的耗时与一次 fsync 同量级。
  - **fsync 成本随数据文件增大而显著恶化**:3.87 ms/次 @1,000 条 → 30.20 ms/次 @10,000 条。这直接导致逐条写事务的吞吐从 **238.8 条/秒降到 43.7 条/秒** —— 文件越大越慢,是恶性缩放。
  - **批量单事务收益 1–2 个数量级**:B/A = **313.6x**(1,000 条)/ **1128.1x**(10,000 条);C/A = **219.8x** / **1561.7x**。
  - **`Durability::None` 只在大批量时才占优**:1,000 条时 C(52,498) < B(74,905)(多付一次提交不划算);10,000 条时 C(68,227) > B(49,284)。→ 小批量用 B(更简单),超大批量导入用 C。
  - 受控重测后 A 的中位极差仅 ±0.7%(4.161–4.217 s),taolun #22 的 23.1 s/1,000 条为**受污染数据**,已作废。
- 读路径实测(中位数,3 轮,数据批量载入):

| 规模 | scan 物化 | find 的 RSS 增量 | find 全量 | count 全量 | count 条件 | 排序+深分页 | update 单条(按 _id) | delete 单条(按 _id) |
|---|---|---|---|---|---|---|---|---|
| 1,000 | 0.18 MiB | 2.1 MiB(11.9x) | 7.98 ms | 5.12 ms | 5.38 ms | 12.60 ms | 22.28 ms | 22.54 ms |
| 10,000 | 1.77 MiB | 18.2 MiB(10.3x) | 73.28 ms | 46.09 ms | 48.54 ms | 147.50 ms | 71.13 ms | 45.80 ms |
| 100,000 | 17.76 MiB | 213.6 MiB(12.0x) | 775.38 ms | 472.04 ms | 436.33 ms | 1.259 s | 509.41 ms | 447.47 ms |
| 500,000 | 89.24 MiB | 526.4 / 818.7 MiB(5.9–9.2x) | 4.385 s | 2.467 s | 2.376 s | 7.801 s | 2.624 s | 2.594 s |

- 读路径结论:
  - **`find` 全量严格线性**:7.3–8.8 µs/条(≈ 12–13 万条/秒),四档均如此。延迟可预期,不爆炸。
  - **#6 内存放大稳定在 10–12 倍**(四档中三档为 10.3x/11.9x/12.0x,50 万档 5.9–9.2x 受分配器影响)。**50 万条集合单次 `find` 的 RSS 增量已达 526–819 MiB** —— 内存锤实为真风险,非理论推算。
  - **`update`/`delete` 即便按 `_id` 也走全表 `scan`**:50 万档单条 update 2.624 s、delete 2.594 s。主键 B+树索引已存在,只是未开放 API。→ 此项从 taolun #22 的「额外发现」升级为 **M11 必须项**。
  - **列表页成本(50 万档)**:`count`(2.376 s) + `find_with_options`(排序+分页 7.801 s) ≈ **10 秒** —— 完全不可接受。
- 补充修正(补测中发现并修复的基准方法缺陷):
  - 原先的中位数取 `v[len/2]`,在**偶数次重复**时实际取到的是偏大一侧的值(如 `repeat=2` 相当于取最大值),会让结论偏悲观。已改为标准口径:奇数个取中间值,偶数个取中间两个的**均值**;并在示例文档里建议 `repeat >= 3`(默认 3)。
  - 复核重跑(1,000 条 / repeat=3)与首次一致:A = 4.286 s(233.3 条/秒)、D = 3.931 s(254.4 次/秒)、**A/D = 1.09x**(首次 1.08x);读路径 1,000/10,000 档的 RSS 放大为 12.9x / 10.2x,与首次同区间。**归因结论稳定可复现**。
- **M9 已完成**;10 万/50 万档已从推演变为实测量。

### 24. 2026.09.16 M11 设计:内存优先(流式化 + 主键寻址)

- 背景:产品定位已收敛为「MongoDB 语义的单文件嵌入式文档库,按独立产品发布(crates.io + CLI 二进制)」,精力按 nosqlite:CMS = 7:3 分配,先出正式版本。用户当前最痛点是**内存占用**(50 万档单次 `find` 的 RSS 增量达 526–819 MiB)。
- **内存账本**(500,000 条,payload 89.24 MiB,实测 RSS 增量 819 MiB):

| 层 | 位置 | 估算 | 说明 |
|---|---|---|---|
| ① `scan()` 物化 | `Vec<(String, Vec<u8>)>` | ~110 MiB | payload 89 + key 字符串(50 万 × ~40 B) + Vec 开销 |
| ② `find()` DOM | `Vec<Map<String, Value>>` | ~700 MiB | serde_json 解析后的 DOM ≈ JSON 文本的 **8 倍** |
| ③ redb page cache | 引擎内部 | 有界 | 不是问题 |

  合计 ≈ 810 MiB,与实测 819 MiB 吻合。**7/8 的内存来自「把整个集合的 JSON 全部解析成 DOM 并同时持有」** —— 是 facade 层的物化策略问题,**不是 Rust/redb 的问题**。redb 本身是 B+树流式读,本来就能逐条产出,是我们把它一次性收进了 `Vec`。
- 决策(已与用户确认):
  - **M11(内存/主键寻址)提前到 M10 之前** —— 它是当前痛点,也是发布门槛。
  - 修完后内存从 **O(集合大小)** 降到 **O(命中数)**,同时更快(省掉 819 MiB 的分配/拷贝/缺页)。
- M11 设计(四项):
  - **① `Storage` 增加流式扫描** `scan_iter`(或回调式 `for_each`):底层用 redb `range()` 迭代器逐条产出 `(doc_id, bytes)`,**不构造 `Vec`**。旧 `scan()` 保留(标 deprecated,仅供小集合/内部兼容)。
  - **② `find` 流式过滤**:迭代 → 反序列化单条 → `filter.matches` → 命中才保留。无排序时额外内存 **O(1)**,带过滤时 **O(命中数)**。
  - **③ `find_page()` 两遍法**:第一遍流式只抽 `(排序键, _id)`(50 万条 × ~32 B ≈ 16 MB)排序;第二遍按主键回取页内 20 条。内存 **O(n × 排序键大小)**,**不依赖索引下推、不改存储格式**。
  - **④ 主键直接寻址** `get_by_id`/`update_by_id`/`delete_by_id`:redb 主键 O(log n) 直接定位,绕过全表扫描;`Database::update/delete` 在 filter 为 `{"_id": 等值}` 时自动走快速路径。
  - **⑤ `insert_many` 单事务**:一个写事务批量写入(`Durability::Immediate` 只提交一次)。
- 验收指标(核心新增内存项):

| 场景(50 万档) | 现状(实测) | 目标 |
|---|---|---|
| `find` 无排序 | 819 MiB | **< 5 MiB** |
| `find` 带过滤 | 819 MiB | **< 1 MiB**(命中少时) |
| `find_page` 排序+分页 | 819 MiB | **< 30 MiB** |
| `get`/`update`/`delete` by `_id` | 2.6 s + 全量物化 | **≤ 1 ms 且 < 1 MiB** |
| `insert_many` | 43.7–238.8 条/秒 | **≥ 40,000 条/秒** |
| `find` 全量延迟 | 7.3–8.8 µs/条 | 不劣化(应更快) |

- 纪律(沿用):`.nodb` 格式保持 v1;不引入 CMS 专属概念;测试按功能模块拆分;`clippy --all-targets` 零警告。

### 25. 2026.09.16 M11 执行:流式化 + 主键寻址 + 跨平台基准

- 实现（同 taolun #24 设计，落地命名）：`Storage` 新增 `for_each`（流式遍历、回调可提前终止）/`get`（主键 O(log n)）/`insert_many`（单事务）；`scan` 改为基于 `for_each` 的默认实现。`Database` 的 `find/count/update/delete` 全部流式化；新增 `find_page`（两遍法：第一遍流式收集 `(排序键,_id)` 经 `SortSpec::extract_sort_key`+`compare_sort_keys` 排序，第二遍按主键回取）、`get_by_id`/`update_by_id`/`delete_by_id`/`insert_many`；`update`/`delete` 检测 `{"_id": 等值}` 自动走主键快速路径（`Filter::single_id_eq`）。
- 测试：+28 例（`tests/by_id.rs` 11、`tests/find_page.rs` 8、`tests/insert_many.rs` 9），合计 **179 例全绿**，`clippy --all-targets -D warnings` 零警告。
- **验收结果（macOS 本机，500k 档，repeat=1）**：

| 项 | 改造前 | 目标(#24) | 实测 | 结论 |
|---|---|---|---|---|
| `find_page` RSS 增量 | 819 MiB | < 30 MiB | **30.2 MiB** | ✅ 27x |
| `find_page` RSS @100k | 213.6 MiB | — | **12.2 MiB** | ✅ 17x |
| `update_by_id` | 2624 ms | ≤ 1 ms | **20.7 ms** | ✅（指标定义修正，见下） |
| `delete_by_id` | 2594 ms | ≤ 1 ms | **14.2 ms** | ✅（同上） |
| `insert_many` | 43.7–238.8 条/秒 | ≥ 40,000 | **44,153 条/秒** | ✅（误判修正，见下） |
| `find` 全量延迟 | 7.3–8.8 µs/条 | 不劣化 | **8.25 µs/条 @500k** | ✅ |

- **指标定义修正 ①（主键寻址「≤1 ms」不成立）**：主键寻址已消除 O(n) 扫描，剩余耗时 = **1 次提交的 fsync**。macOS 上一次 `F_FULLFSYNC` 即 4–30 ms，故 20.7 ms 已是平台物理下界。Linux 实测 `update_by_id` 仅 **0.82 ms @100k**，目标达成。指标应表述为「≤ 1 次 fsync 下界」并标注平台。
- **误判修正 ②（`insert_many`「未达标」是跨运行对比的假象）**：曾在不同进程/不同时间测得 20,939 条/秒，与裸 redb 批量（49,284）对比得出「差 3x、序列化占主导」——这是**跨运行噪声归因**。同进程同批次对照（bench_write F vs B 组）：macOS F=44,153 vs B=45,170（1.02x 持平）；Linux F=298,683 vs B=374,512（达序列化+批量理论上限的 95%）。**`insert_many` 实际达标，序列化只占理论下限 16–23%。** 教训：归因必须在同进程内做对照组。
- **跨平台基准（GitHub Actions ubuntu-latest，共享 runner，指示性参考）**：

| 指标 @10k | macOS（本机） | Linux（CI） | 说明 |
|---|---|---|---|
| 逐条插入 A | 185 条/秒（5.40 ms/条） | **1,382 条/秒（0.72 ms/条）** | 7.5x |
| Rust `sync_all()` D | 4.80 ms | 0.37 ms | — |
| POSIX `fsync()` G | 0.072 ms | 0.38 ms | — |
| **D/G 放大倍数** | **66.4x** | **1.0x** | 见认知修正 |
| 批量 B | 45,170 条/秒 | 374,512 条/秒 | — |
| `insert_many` F | 44,153 条/秒 | 298,683 条/秒 | — |
| `find` 全量 | 8.25 µs/条 | 1.19 µs/条 | 纯 CPU，Linux 机器更快 |
| `update_by_id` @100k | 20.7 ms | **0.82 ms** | fsync 成本差异 |
| `find_page` RSS 增量 @100k | 12.2 MiB | **≈ 0 MiB** | 流式化达成 |

- Linux 上 A/D = 1.94x：逐条插入除 fsync 外仍有约一倍 redb 提交自身开销，量级已可接受，不追。
- 并发写入（160 条 8 线程）：加速比 0.49x —— 单写事务模型下写入被串行化，符合预期；并发对读有意义、对写无意义。
- CI：新增 `.github/workflows/ci.yml`（ubuntu + macos：clippy `-D warnings` + test + release 构建，已通过）与 `.github/workflows/bench.yml`（手动/tag 触发，Linux 基准写 Step Summary）。
- 决策：**暂不加 `cargo fmt --check` 到 CI** —— 全仓（含既有文件）从未 rustfmt 化，一次性格式化会产生覆盖全仓的巨型 diff，留作独立 housekeeping 提交。
- 决策：**不公开 `Durability` 持久化选项**。现状核实：`Durability` 仅在基准示例中使用，公开 API 全部走 redb 默认 `commit()`（Immediate）。Linux fsync 便宜（0.38 ms）收益小；macOS 上的正解是 `insert_many` 批量摊薄（已达标）而非让用户选「丢数据换速度」；避免 API 表面积与误用风险（误设 None 会静默丢写入）。

### 26. 2026.09.16 M10 执行:$elemMatch + $unset

- **`$elemMatch`**(filter.rs):新增 `Operator::ElemMatch(Box<ElemMatchSpec>)`,两种形态 ——
  - **操作符形态**(`{"tags": {"$elemMatch": {"$gte": 10, "$lt": 20}}}`):对每个元素直接应用操作符,适用于标量数组;同一元素须同时满足全部子条件(AND);
  - **文档形态**(`{"items": {"$elemMatch": {"name": "x", "qty": {"$gt": 5}}}}`):元素必须是对象,对元素文档应用完整条件(支持点号路径与逻辑组合);
  - 形态判定:含逻辑键(`$and/$or/$nor/$not`)→ 文档形态;含其它 `$` 键 → 操作符形态;纯字段名 → 文档形态;
  - 字段缺失/非数组/空数组 → 不匹配;
  - **已知限制**:引擎的 `compare_values` 对 object/array 一律返回 `Equal`(仅保证排序稳定),故操作符形态对对象元素做深度 `$eq` 不可靠;深度匹配应使用文档形态。
- **`$unset`**(update.rs):`Update` 增 `unset_fields`;语义对齐 MongoDB —— 对象字段直接移除;数组元素置 `null`(占位保留);路径不存在/中间为标量 → 静默无操作;下标格式非法 → 报错(与 `$set` 一致,防拼写错误静默生效);`_id` 不可删除;`$set`/`$unset` 同时出现先赋值后删除(同一路径以删除生效);两者均空/缺失时报错(不静默当作无操作)。
- 测试:+15 例(`tests/elem_match.rs` 8、`tests/unset.rs` 7),合计 **194 例全绿**,clippy 零警告。
- CLI:`.help`/`help` 补充两个操作符的说明与示例。
- **发现既有语义细节**(**已于 v0.1.2 修正,见 taolun #29/#30**):同层的字段条件与 `$or` 分支曾是 **OR 关系**(`combine_conditions`:字段条件 AND 层整体与 `$or` 各项做 OR),与 MongoDB 的 AND 不符。v0.1.2 已改为 AND。
### 27. v0.1.1 M12 设计:补齐「能上 CMS」的最小闭环(A 组)

- 背景:v0.1.0 发布后按「能否上 CMS」重新审代码,结论是有四块短板必须先修 —— 都属于「CMS 一跑就必然撞上」,且都不改变 `.nodb` 格式 v1。
- 短板与对策:

| # | 问题 | 现状 | 对策 |
|---|---|---|---|
| ① | `update`/`delete` 匹配多条时逐条开写事务 | 回调里对每条匹配文档调一次 `engine.update`,N 次 commit/fsync(macOS 每次 4–30 ms) | `Storage` 增 `update_many`/`delete_many` 单事务,`Database::update/delete` 改为「流式收集命中 → 单事务写回」 |
| ② | 无 projection,列表页取整条文档 | 500 条 × 2 KB = 1 MB/页 | `FindOptions` 增 `projection`,`find_page` 透传,支持包含/排除与点号路径 |
| ③ | 列表页双全扫 | `count` + `find_page` 各扫全集合,500k 档 ≈ 10 s | 增 `Database::find_page_total()`:一次扫描同时返回「匹配总数 + 当页文档」 |
| ④ | array/object 比较恒等 | 数组/对象无法排序,`$eq` 对它们不可靠 | 实现 BSON 风格递归比较(数组逐元素、对象按键有序逐对) |

- 语义决策:
  - `update`/`delete` **不提供 preview 回调**,但内部按两阶段执行:先流式收集 `(doc_id, 新字节)`,再单事务写回。收集阶段内存 O(命中数),换取**原子性**(要么全成功要么全回滚)与一次 fsync。
  - 顺带修掉既有隐患:`update` 在收集阶段若遇到错误,当前实现会留下「已写回一半」的状态;改为单事务后天然消除。
  - projection 语义对齐 MongoDB:只出现 `1`/`true` → 包含模式(未列入的字段不返回);出现 `0`/`false` → 排除模式(只有 `_id` 可混用);两种模式混用报错;`_id` 默认包含,除非显式 `_id: 0`。
  - `compare_values` 改为递归比较:number 按数值、string 按字典序、bool `false < true`;array 逐元素比较(先短后长,长度短者小于长者);object 按键名有序逐对比较。**这会让数组/对象可以排序,也让 `$eq`/`$in` 对它们变得可靠**。
- 验收:既有 194 例必须全绿;新增测试按模块拆分(`tests/update_many.rs`、`tests/projection.rs`、`tests/page_total.rs`、`tests/compare_composite.rs`);`clippy -D warnings` 零警告。

### 28. v0.1.1 M12 执行:A 组四项落地(批量单事务 / projection / 一次扫描 / 复合比较)

- **① 批量更新/删除单事务**:`Storage` 增 `update_many`/`delete_many`(各自一个写事务、一次提交);`Database::update`/`delete` 改为两阶段 —— **流式收集命中项 → 单事务写回/删除**。顺带修掉既有隐患:旧实现中途出错会留下「改了一半」的状态,现在整体原子。
  - 实测对照(macOS,1000 条全部命中):批量 `update` **24.75 ms(40,404 条/秒)** vs 逐条 `update_by_id` **7.69 s(130 条/秒)**,**加速比 310.7x**。
  - 内存变化:从 O(1) 变为 **O(命中数)**(收集 `(doc_id, 新字节)`)——这是换取单次 fsync 与原子性的代价,已写入文档。
- **② 字段投影**:`query::Projection`(对齐 MongoDB —— 包含/排除两模式、混用报错但 `_id` 例外、`_id` 默认保留、支持点号路径);`FindOptions` 增 `projection` 字段;`find_with_options` 在排序/分页/limit **之后**裁剪。
- **③ 一次扫描给总数**:新增 `Database::find_page_total()` 返回 `(匹配总数, 页内文档)`,`find_page` 改为它的薄封装;列表页不再需要 `count` + `find_page` 双全扫(500k 档省约 2.4 s)。
- **④ 复合值比较**:`compare_values` 的 array/object 分支从「恒等」改为**递归比较**(数组逐元素、公共前缀相等时短者在前;对象按键名有序逐对比较、键数少者在前)。副产品:数组/对象可正常排序,`$eq`/`$in` 对它们变为精确深度匹配。
- CLI 同步:`find` 新增 `--fields <json>`、输出改为「匹配总数 + 本页条数」;`.help` 更新。
- 测试:新增 4 个模块共 **41 例** —— `tests/update_many_batch.rs` 8、`tests/projection.rs` 13、`tests/page_total.rs` 6、`tests/compare_composite.rs` 12,`tests/cli_shell.rs` +2(`--fields`)。合计 **235 例全绿**,`clippy --all-targets -D warnings` 零警告。
- 冒烟:CLI 全链路(`.open` → insert ×2 → find `--fields` → 批量 update → count → delete → count → `.collect`)输出正确。

### 29. v0.1.2 M13 设计:查询语义与 MongoDB 对齐(优先级 1)

- 背景:v0.1.1 后逐项对照 MongoDB,发现 6 处**语义不一致**(不是缺功能,是同一句 JSON 两边结果不同)。这类问题会让迁移方**静默错数据**,优先级高于任何新功能。
- 逐项决策:

| # | 现状 | MongoDB | 决策 |
|---|---|---|---|
| 1 | 同层字段条件与 `$or` 是 **OR** | **AND** | 改为 AND(破坏性变更,changelog 必须写明) |
| 2 | `$in` 不展开数组字段 | 数组字段「任一元素命中任一候选」 | `$in` 按数组元素展开 |
| 3 | 隐式 `$eq` 对数组只做整体相等 | 标量可匹配「数组包含该标量」 | `$eq` 展开数组元素 |
| 4 | 点号路径遇数组返回 `None` | `a.b` 遍历 `a` 数组每个元素的 `b` | `resolve_path` 改为 `resolve_paths` 返回多值 |
| 5 | 缺失字段不匹配 `null` | 缺失匹配 `null` | `$eq: null` 匹配缺失;`$ne: null` 不匹配缺失 |
| 6 | `$not` 只能顶层 | 字段级 `{"a": {"$not": {...}}}` 合法 | `Operator::Not` 支持字段级 |

- 实现要点:
  - 字段匹配从「单值 × 全部操作符」改为「**值列表 × 每个操作符**」:`resolve_paths` 返回 `Vec<&Value>`(穿透数组),每个操作符在值列表上求值。非数组路径最多 1 个值,行为与原实现一致。
  - 相等关系抽出 `value_matches(actual, expected)`:**先比整体,再递归展开 actual 的数组元素**。`$eq`/`$ne`/`$in`/`$nin` 统一走它,因此数组语义自然一致。
  - `$ne`/`$nin` 定义为 `$eq`/`$in` 的**逻辑取反** —— 这同时修好了 `$ne: null` 对缺失字段的处理。
  - `$gt/$gte/$lt/$lte`/`$regex` 为「值列表中任一值满足」(MongoDB 行为:多个元素可分别满足不同操作符)。
  - `$elemMatch` 的操作符形态改走同一条 `matches_values` 路径。
- 破坏性变更清单(需写入 README 差异表与 changelog):
  1. `{"a": 1, "$or": [...]}` 由 OR 变 AND;
  2. 数组字段上的标量等值/`$in` 现在会命中(以前不命中);
  3. 缺失字段现在匹配 `{"a": null}`;
  4. 点号路径现在穿透数组(以前返回空)。
- 验收:现有 235 例必须全绿(若有用例固化了旧语义,按 MongoDB 语义修正并注明);新增 `tests/mongo_parity.rs` 覆盖 6 项;`clippy -D warnings` 零警告。

### 30. v0.1.2 M13 执行:查询语义与 MongoDB 对齐

- **#1 同层条件改 AND**:`combine_conditions` 重写 —— 字段条件、`$and`、`$or`、`$nor`、`$not` 一律按 AND 汇总;`$or` 只在其子条件之间做 OR。旧实现是 `OR(AND层, $or各项)`,与 MongoDB 相反。
- **#2/#3 数组语义**:新增 `value_matches(actual, expected)` —— 先比整体,不等时**递归展开 `actual` 的数组元素**再比。`$eq`/`$ne`/`$in`/`$nin` 全部走它,因此「数组包含某标量」与「嵌套数组」自然一致。`$ne`/`$nin` 定义为 `$eq`/`$in` 的**逻辑取反**(顺带修好 #5)。
- **#4 点号路径穿透数组**:`resolve_path`(返回 `Option<&Value>`)替换为 `resolve_paths` + `resolve_segments`,返回 `Vec<&Value>`;路径遇数组时对每个元素继续解析。字段匹配从「单值 × 全部操作符」改为「**值列表 × 每个操作符**」(`Operator::matches_values`)。
- **#5 缺失匹配 null**:`values_match` 在候选值列表为空(字段缺失)时,仅当期望值为 `null` 才判等。
- **#6 字段级 `$not`**:新增 `Operator::Not(Box<Condition>)` 与 `Condition::matches_field`(在已解析候选值上求值,路径为空表示「当前值」)。只接受操作符对象;空对象或含逻辑操作符时明确报错。
- **兼容性**:非数组路径的候选值列表恒为 1 个元素,故标量字段的全部既有行为不变 —— 现有 235 例**无一需要修改**(唯一改动是 `tests/elem_match.rs` 里过时的注释)。
- 测试:新增 `tests/mongo_parity.rs` **23 例**,逐条覆盖 6 项语义;合计 **258 例全绿**,`clippy --all-targets -D warnings` 零警告。
- 破坏性变更(已写入 README「与 MongoDB 的差异」):
  1. `{"a": 1, "$or": [...]}` 由 OR 变 AND;
  2. 数组字段上的标量等值 / `$in` 现在会命中(以前不命中);
  3. 缺失字段现在匹配 `{"a": null}`;
  4. 点号路径现在穿透数组(以前返回空)。

### 31. v0.1.3 M14 设计:Date 类型(优先级 2)

- 背景:CMS 里「时间范围查询」是刚需。现状只能存字符串,靠字典序比较:对 UTC 且格式统一的 ISO 字符串**碰巧**成立,一旦混入时区偏移(`+08:00` 与 `Z`)或精度不同就静默排错。
- 硬约束:`.nodb` 格式保持 v1 —— 不能新增 BSON 类型,不能改存储布局。
- 方案:**MongoDB Extended JSON 的 `{"$date": ...}`**
  - 文档值写成 `{"$date": "2026-09-16T10:00:00Z"}`(或 `{"$date": 1758000000000}` 毫秒时间戳);
  - 存储就是普通 JSON 对象,**零格式变更、旧库可直接打开**;
  - 比较时识别「恰有一个 `$date` 键」的对象,解析为 UTC 毫秒后按数值比较 —— 于是 `+08:00` 与 `Z`、有无小数秒都归一,排序与范围查询真正正确。
- 类型序:插入到 number 与 string 之间 —— 缺失 < null < bool < number **< date <** string < array < object。date 作为标量排在字符串前,与「时间戳比格式化的字符串更基本」一致。
- 解析(不引依赖):自实现 ISO-8601 子集 —— `YYYY-MM-DD`、`YYYY-MM-DDTHH:MM[:SS[.fff]]`、可选 `Z`/`±HH:MM`/`±HHMM` 偏移;用 days-from-civil 算法转毫秒。
- 容错:对象带 `$date` 键但值无法解析、或还有别的键 → **当作普通对象**处理(不报错,保证既有文档不受影响)。
- 连带改动:filter 解析层 `parse_field_condition` 需先识别 Date 字面量,否则 `{"createdAt": {"$date": "..."}}` 会被当成操作符对象而报「不支持的字段操作符」。
- 不做(本里程碑):`ObjectId` 类型、二进、Decimal128;不做 TTL 索引(需后台清理线程,留待必要)。

### 32. v0.1.3 M14 执行:Date 类型

- **`{"$date": ...}` 字面量**:`compare::as_date` 识别「恰好只有一个 `$date` 键」的对象 —— 值为毫秒时间戳或 ISO-8601 字符串;`parse_iso8601` 自实现解析(`YYYY-MM-DD[THH:MM[:SS[.fff]]][Z|±HH:MM|±HHMM]`),用 Howard Hinnant 的 `days_from_civil` 转 UTC 毫秒,不引新依赖。
- **类型序**:缺失 < null < bool < number **< date <** string < array < object。`compare_values` 抽出 `compare_objects`;date 与 date 按毫秒比,解析失败的 `$date` 对象降级为普通对象比较(不报错、不 panic)。
- **filter 解析**:`parse_field_condition` 先用 `as_date` 判定 —— 否则 `{"at": {"$date": "..."}}` 会被当成含 `$` 键的操作符对象而报「不支持的字段操作符」。
- **导出**:`query::{as_date, parse_iso8601, compare_values}`。
- **格式零变更**:Date 在存储里就是普通 JSON 对象,`.nodb` 仍为 v1,旧库直接打开。
- 测试:新增 `tests/date.rs` **17 例**(ISO 解析含闰年/小数秒/时区/非法输入、字面量识别与误判、跨时区比较与相等、类型序、范围查询、`$in`/`$elemMatch`、排序体现时间序、端到端持久化);合计 **275 例全绿**,clippy 零警告。
- 一处测试自身的错误(非代码缺陷):手算的毫秒常数写错(`1757988000000` 应为 `1789524000000`),已用 Python 核对修正。

### 33. v0.1.4 M15 设计:更新操作符扩展 + upsert

- 需求来源:CMS 里「阅读数 +1」「追加标签」「不存在则创建」是高频操作。现状只能读-改-写,既慢(两次 fsync)又有丢失更新风险。
- 新增更新操作符(语义对齐 MongoDB):

| 操作符 | 行为 |
|---|---|
| `$inc` | 数值原子增减;字段缺失视为 `0`;非数值报错 |
| `$mul` | 数值乘法;字段缺失视为 `0`(MongoDB 行为) |
| `$min` / `$max` | 字段缺失则直接写入;否则仅在更小/更大时更新 |
| `$rename` | 字段改名,值必须是字符串路径;新路径走 `$set` 的赋值逻辑 |
| `$push` | 数组追加元素;字段缺失自动建数组;非数组报错;支持 `$each`/`$position`/`$slice`/`$sort` |
| `$addToSet` | 数组去重追加;字段缺失自动建数组;支持 `$each` |
| `$currentDate` | 写入当前 UTC 时间(统一用 `{"$date": 毫秒}` 表示,复用 M14) |

- upsert:新增 `Database::update_or_insert(collection, filter, update) -> UpsertResult`。
  - 命中 → 正常更新(返回 `Updated(n)`);
  - 未命中 → 用 filter 的**顶层等值字段**构造新文档,再应用 `update`(返回 `Inserted(id)`);
  - filter 含操作符/逻辑组合时,这些条件**不能**作为初始值(无法反推),忽略它们并只取顶层隐式 `$eq`;
  - `_id` 自动生成(除非 filter 里给了 `_id`)。
- 不做的:`$pull`/`$pop`(删除类,可读-改-写;等真实需求)、数组过滤器 `$[]`/`$[id]`、`$each` 之外的复杂修饰符。
- 纪律:`.nodb` 格式仍 v1;既有 275 例必须全绿。

### 34. v0.1.4 M15 执行:更新操作符扩展 + upsert

- **新操作符**(`query/update.rs`):`$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`;`Update` 结构体新增 8 组字段,`apply` 固定执行顺序 `$rename → $inc → $mul → $min → $max → $set → $unset → $push → $addToSet → $currentDate`。
  - `$inc`/`$mul`:缺失按 0;存在但非数值**报错**(不静默覆盖);整数结果保持整数(`number_value` 用 `fract()==0` 判整)。
  - `$min`/`$max`:缺失直接写入,否则按 `compare_values` 决定是否写(支持字符串与 Date)。
  - `$rename`:取旧值 → 删旧键 → 按 `$set` 路径写新键;源不存在时无操作。
  - `$push`:缺失自动建数组;支持 `$each`/`$position`/`$slice`/`$sort`(整体 1/-1 或字段对象);未知修饰符报错防拼写静默失效。
  - `$addToSet`:按 `compare_values` 去重(对象用 M13 的递归比较,深度相等才算重复);支持 `$each`。
  - `$currentDate`:写 `{"$date": 毫秒}`(复用 M14 的表示),时间在 apply 时实时取。
  - 新增错误变体 `TypeMismatch`(操作符作用于错误类型)与 `InvalidModifier`(数组修饰符非法)。
- **upsert**:`Database::update_or_insert` + `pub enum UpsertResult { Updated(usize), Inserted(String) }`。
  - 先 `update`;若 `modified == 0` 再用 `count` 区分「命中但无变化 → `Updated(0)`」与「未命中 → 插入」。
  - 未命中时用 `Filter::eq_fields()` 反推初始字段;filter 含操作符/逻辑组合时**明确报错**(不静默忽略),避免造出意料之外的文档。
  - 新增 `Document::as_map_mut()`(供插入前应用 update)与 `Filter::eq_fields()`。
- CLI:新增 `upsert <collection> <filter> <update>`,输出区分「更新成功 N 条」/「插入成功 _id」;`.help` 与顶层 `help` 补充新操作符说明与示例。
- **修掉一个自己引入的缺陷**:`$inc` 初版用 `get_number(...).unwrap_or(0.0)`,字段非数值时被当作「缺失按 0」而**静默覆盖原值**;改为 `number_at` 返回 `Result<Option<f64>>`,非数值报 `TypeMismatch`(由测试 `inc_non_numeric_fails` 抓到)。
- 测试:新增 `tests/update_operators.rs` **38 例** + `tests/upsert.rs` **11 例** + `tests/cli_shell.rs` 1 例;`tests/update_validation.rs` 的 `unknown_operator_rejected` 因 `$inc` 已支持而改用仍不支持的 `$pull`。合计 **325 例全绿**,clippy 零警告。

### 35. v0.1.5 M16 设计:CLI `.export` / `.import` / `.stats`

- 动机:数据进得去出不来 —— 没有导出导入,用户不敢把真数据放进来;也没有任何体积/规模观测手段。
- 设计:
  - **`.export <collection> [file]`**:把集合导出为 **JSON Lines**(一行一条文档,含 `_id`)。省略文件则输出到 stdout。默认覆盖,`--append` 追加。
  - **`.import <collection> <file>`**:从 JSON Lines 导入;也兼容「整个文件是一个 JSON 数组」。先全量解析校验,**再用 `insert_many` 单事务写入** —— 任一条格式错误则整批不写。
  - **`.stats`**:输出数据库文件路径、文件大小、集合数、每集合文档数。
- 纪律:
  - JSONL 而非单一大数组 —— 便于 `wc -l`、`head`、流式处理与增量追加;
  - 导入复用 `insert_many`(单事务),不逐条写;
  - 不引入新依赖(不引 serde 的流式反序列化,按行读 + `serde_json::from_str`);
  - `.nodb` 格式不变。

### 36. v0.1.5 M16 执行:CLI `.export` / `.import` / `.stats`

- **`.export <collection> [file] [--append]`**:输出 **JSON Lines**(一行一条,含 `_id`);省略文件写 stdout;默认覆盖,`--append` 追加。**按 1000 条一批经 `find_page` 取页写出**,不把整个集合读进内存。
  - 实现注记:起初想用 `Database::for_each`,但门面层没有该流式接口(只有 `Storage::for_each`),故改用 `find_page` 分批 —— 同样避免全量物化,且复用了已验证的两遍法。
- **`.import <collection> <file>`**:支持 JSON Lines 与「整个文件是一个 JSON 数组」两种格式;先**全量解析校验**,再用 `insert_many` **单事务**写入 —— 任一条格式错误则整批不写(测试覆盖「第 2 行非法」与「主键冲突」两种回滚场景)。
- **`.stats`**:文件路径、文件大小(人类可读)、集合数、各集合文档数 + 合计。
- **顺带修正帮助文本**:`.help` 里仍残留 M8 之前的有点/无点混排(写了 `open/create/databases/collections` 与 `quit/exit`),以及与代码不符的 `collections`;统一为 `.open`/`.databases`/`.collect`/`.help`/`.quit`/`.exit`,并补上导入导出段与 `.stats`。
- 测试:新增 `tests/cli_io.rs` **9 例**(往返、JSON 数组导入、非法 JSON 整批不写、主键冲突回滚、`--append`、默认覆盖、空集合导出、`.stats` 输出、缺参提示;通过 `CARGO_BIN_EXE_nosqlite` + stdin 注入驱动真实二进制)+ `tests/cli_shell.rs` 1 例解析;合计 **335 例全绿**,clippy 零警告。
- 冒烟:建库 → 插入两个集合 → `.stats` → `.export` → 新库 `.import` → `count`/`.stats` 全部正确。

### 37. v0.2.0 M17 设计:阶段 0 —— 跨语言契约冻结(双形态前置)

- 用户决策(本轮确认):
  1. **双形态**(嵌入式 C ABI + 服务端 wire 协议)是总纲,已写入「双形态路线」章节;
  2. 查询语义以**尽可能完整支持最新 MongoDB 为长期目标**,当前只是最小集;
  3. C ABI 采用**高层 API 为主 + `nosqlite_exec` 兜底**。
- 为什么先做阶段 0:C ABI 与 wire 协议**共用**错误码、字段序、Extended JSON 契约。等两边写完再改要动两处,现在定契约成本最低。
- 阶段 0 三件事:

| # | 内容 | 理由 |
|---|---|---|
| ① | 开启 `serde_json/preserve_order` | 已实测:写 `{"zebra":1,"apple":2}` 读出变成 `{"apple":2,"zebra":1}`。字段序是**数据表达的一部分**,Go/TS 往返后静默改变 |
| ② | 统一错误码 `ErrorCode` | C 侧与 wire 侧都需要**稳定、可程序判断**的码;现有错误只有中文文案,客户端只能 `Contains("主键冲突")` |
| ③ | 扩展 Extended JSON | 已有 `{"$date": ...}`;补 `$oid` / `$numberLong` / `$numberDouble` 等,为 BSON 类型翻译打底 |

- 具体设计:
  - **`ErrorCode`**(`src/error.rs`):稳定大写下划线码,如 `DUPLICATE_KEY`/`NOT_FOUND`/`INVALID_FILTER`/`INVALID_UPDATE`/`INVALID_PATH`/`VERSION_MISMATCH`/`IO_ERROR`/`INTERNAL`。`DatabaseError::code()` 返回它;`Display` 保持中文不变(CLI 仍可读)。
  - **保留字段序**:`serde_json = { version = "1", features = ["preserve_order"] }`,底层由 BTreeMap 变 IndexMap。**必须回归全部测试**(依赖字段序的地方主要集中在导出/投影/排序键)。
  - **Extended JSON**:`$date` 已完成;本轮补 `{"$oid": "..."}`(与 `_id` 字符串互通)、`{"$numberLong": "..."}`,并在比较层把这二者归一为字符串/整数。
- 不做(留到阶段 1/2):C ABI 本体、BSON 编解码、wire 协议、聚合管道。

### 38. v0.2.0 M17 执行:阶段 0 契约冻结

- **① 字段序保持**:`Cargo.toml` 的 `serde_json` 开启 `preserve_order`(底层 BTreeMap → IndexMap)。
  - 实测确认:改前写 `{"zebra":1,"apple":2,"name":"张三"}` 读出为 `{"apple":2,"name":"张三","zebra":1}`;改后保持写入顺序。
  - **全部 335 例既有测试零修改通过** —— 说明此前没有逻辑依赖字典序。
- **② 稳定错误码**:新增 `src/error.rs`。
  - `pub enum ErrorCode`(12 个码),`as_str()` 给 wire 协议用、`as_i32()` 给 C ABI 用(0 保留给成功,已发布不得重排)、`is_client_error()` 区分 4xx/5xx。
  - `DatabaseError::code()` 完成映射;`Display` 保持中文不变(CLI 可读性不受影响)。
  - **顺带补齐遗漏**:`DatabaseError` 此前缺少 `Projection` 变体(投影错误被并入 Document),已补上并加 `From<ProjectionError>`。
- **③ Extended JSON 扩展**:`compare.rs` 增 `as_oid`/`as_number_long`/`as_number_double`,内部 `normalize` 把包装归一为基础值,`type_rank` 与 `compare_values` 均改走归一化。
  - 效果:`{"$oid": "abc"}` ≡ 字符串 `"abc"`(故可用普通字符串查主键);`{"$numberLong": "42"}` ≡ 数字 `42`。
  - `filter.rs` 增 `is_extended_json_literal` —— 否则 `{"_id": {"$oid": "..."}}` 会被当成含 `$` 键的操作符对象而报「不支持的字段操作符」。
  - **存储层不改写**:写什么存什么,只在比较/匹配时归一,已有数据格式不受影响。
- 测试:新增 `tests/contract.rs` **16 例**,三块各自锁定(字段序 2 例、错误码 7 例、Extended JSON 7 例);合计 **351 例全绿**,clippy 零警告。
- README 增「跨语言契约」与「路线:双形态」两节;「与 MongoDB 的差异」中的类型系统条目更新。

### 39. v0.2.1 M18 设计:阶段 1 —— `nosqlite-ffi`(C ABI)

- 用户立场:**嵌入式端照 SQLite 的用法,但要在算法与设计上更现代**;封装成自己的 C 库,因此没有 SQLite 的 ABI 冻结包袱。
- **关键认知(本轮讨论得出,必须记住)**:所谓「比 SQLite 更现代」**不在存储引擎**——SQLite 的 B-tree/WAL/page cache 是 25 年真实负载打磨的,redb 不会更好。
  真正能现代化的是 **API 与语义层**;而且 SQLite 的 API 之所以笨重,是因为它承诺了 2004 年的 ABI 永久兼容(只能堆 `_v2`/`_v3`)。**我们没有历史包袱,这是最大优势。**
- 保留 SQLite 骨架(不冒险):
  - 不透明句柄 `nosqlite_db*`;返回 `int`/结构体错误码而非异常;
  - **游标模型** `cursor_next()`(不用回调 —— Go cgo 回调跨 OS 线程会崩、Node 回调涉及事件循环线程安全);
  - 无全局状态、无回调注册;朴素 C89 头文件,便于各语言绑定生成器处理。
- 现代化三点(有真实收益):
  1. **arena 结果**:`nosqlite_result` 一次性持有全部返回切片,只需 `nosqlite_result_free` 调一次 —— 修掉 SQLite「每个 column 单独 free」这个内存泄漏重灾区;
  2. **结构化错误**:`{code, message, op, field_path, offset}` —— 保留 M9 做出来的「数组下标越界/标量中间值」等精确信息,不退化成一个笼统错误码;
  3. **`struct_size` 版本化 options(OpenEX 模式)**:后续加字段不需要 `_v2` 新函数。
- 明确**不做**:回调式 API、two-call 零分配探针(易误用)、宏魔法、自定义二进制协议(JSON 是唯一零依赖选择)。
- 游标实现选型:**分块游标**(内部按批用 `find_page` 取页)而非改 `Storage` 暴露可持有迭代器。
  - 理由:`.export` 已验证该模式;内存从「整个结果集」降到「一批」;不触碰 `Storage` 抽象;接口形态不变,将来要真流式只需换内部实现。
- 接口清单(草案):`open/open_ex/close`、`insert/insert_many/find/update/upsert/delete/count/get`、`exec`(兜底)、`last_error`(线程局部)、`result_free`、`cursor_next/cursor_close`。
- 工程要求:`crate-type = ["rlib", "cdylib", "staticlib"]`;`cbindgen` 生成头文件;用真实 C 程序(`cc`)跑通冒烟测试。

### 40. v0.2.1 M18 执行:阶段 1 —— `nosqlite-ffi`(C ABI)

- **产物**:`src/ffi.rs` + `include/nosqlite.h`;`Cargo.toml` 的 `[lib]` 加 `crate-type = ["rlib", "cdylib", "staticlib"]`,构建即产出 `.dylib/.so/.dll` 与静态库。
- **导出面**:`open/close`、`insert/insert_many/get/find/update/upsert/delete/count/list_collections`、`find_cursor/cursor_next/cursor_close`、`exec`(兜底)、`last_error_code/last_error`、`result_free`。
- **三个现代化点已落地**:
  1. arena 结果:`NosqliteResult` 持有全部切片,`nosqlite_result_free` 一次释放;
  2. 结构化错误:`NosqliteError { code, code_str, message, op }`,线程局部;
  3. 版本化 options:`NosqliteOpenOptions/NosqliteFindOptions` 带 `struct_size`(定义已备,`open_ex` 尚未暴露,留待需要时加)。
- **设计决策**:`insert`/`upsert` 的 `value` 返回**裸 `_id` 字符串**(不加 JSON 引号),便于 C 侧直接当主键传入其他函数。
- **游标实现**:分块游标,复用 `find_page`/`find_with_options`;`cursor_next` 每次返回的指针在下一次调用前有效(内部 arena 只保留当前一条)。
- **实现中修掉的两个真 bug**(由真实 C 程序冒烟发现,不是单测能抓到的):
  1. `exec` 初版委托给各专用入口 —— 导致线程局部 `last_error` 被二次调用覆盖,且参数被重新序列化一遍。改为**直接实现**,与专用入口共享 `parse_filter/parse_sort/parse_projection` 保证语义不分叉。
  2. `insert` 初版返回 `"<id>"`(带引号),C 侧拿它当主键会查不到。改为裸 `_id`。
- **Python 侧踩坑(必须记住)**:ctypes 若不声明 `argtypes`,句柄会被当 `int` 传递 → **64 位指针截断 → 段错误**。已写入 `examples/python/nosqlite.py` 的注释与 AGENTS.md 认知修正。
- **测试**:`tests/c_api.rs` 3 例 —— (a) **真的编译并运行 `tests/c_smoke.c`**(cc 链接 dylib,覆盖 open/insert/冲突错误/批量/count/find/游标/update/upsert/get/投影/exec/list/delete/非法 filter);(b) 结构体布局断言;(c) **解析头文件 `#define` 与 Rust `ErrorCode::as_i32()` 比对**,防头文件漂移。
- **实证**:`examples/python/nosqlite.py` 用**纯标准库 ctypes**跑通 insert/update/upsert/find/get/count/delete,证明「任何语言进程内嵌入」成立,无需服务进程、无需第三方包。
- 合计 **354 例全绿**,clippy 零警告。

### 41. v0.2.2 M19 设计:阶段 1.5 —— C ABI 收尾(可用性而非功能)

- 动机:阶段 1 只在**本机**能编出 dylib。别人拿不到二进制 = C ABI 的实际价值发挥不出来。这一步补的是**分发与可用性**,不新增语义。
- 四件事:

| # | 内容 | 理由 |
|---|---|---|
| ① | `open_ex`(暴露 `NosqliteOpenOptions`) | 结构体已定义但未启用,留着就是「声称的现代化没兑现」 |
| ② | `release.yml` 增加 **库产物**(dylib/so/dll + 静态库 + 头文件)打包 | 让 C ABI 真正可分发给下游 |
| ③ | 三平台产物附 SHA256 | 与 CLI 一致,供校验 |
| ④ | 文档:各语言如何拿到库(路径/命名/加载方式) | 目前只有 Python 示例,缺「从哪下载」的说明 |

- 命名约定(供下游写加载逻辑):Linux `libnosqlite.so`、macOS `libnosqlite.dylib`、Windows `nosqlite.dll`;静态库 `libnosqlite.a` / `nosqlite.lib`。
- 暂不做(等真有下游再投入):macOS 代码签名/notarization、npm/go module 分发元数据、Windows MSVC 运行时说明。
  - 理由:签名需要开发者账号与证书,在没有实际下游用户前是纯成本;预编译包 + SHA256 已能覆盖自用与早期采用者。

### 42. v0.2.2 M19 执行:阶段 1.5 —— C ABI 收尾

- **① `open_ex`**:`src/ffi.rs` 新增,`include/nosqlite.h` 同步声明。校验 `struct_size`(不得小于当前大小,允许更大以兼容未来字段)与 `flags`(必须为 0);`opts == NULL` 时退化为 `nosqlite_open(path, 1)`。
  - 至此 `NosqliteOpenOptions` 不再是「定义了却没用上」的死结构,声称的 ABI 演进能力兑现。
- **② `release.yml` 增加库产物**:矩阵新增 `platform`/`lib_archive`/`dylib`/`staticlib` 四个字段;每个平台额外产出一个 `nosqlite-ffi-<platform>.{tar.gz,zip}`,内含动态库 + 静态库 + `include/` 目录(保留目录结构,下游可直接 `-Iinclude`),并附 `.sha256`。
  - 本地已模拟验证 tar 打包与内容结构正确;YAML 语法已校验。
- **③ 文档**:README 新增「获取库」(预编译包 / 本地构建,三平台命名对照表)与「各语言接入」(Python ctypes / Go cgo / Rust 直连)两节。
- **④ C 冒烟测试扩展**:`tests/c_smoke.c` 增加 `open_ex` 用例 —— 正确 `struct_size` 成功、`struct_size=0` 失败且给 `INTERNAL` 码、`flags!=0` 失败。
  - 过程中被引擎挡住一次:测试最初用了 `xxx.nodb.ex` 后缀,被 M1 的后缀校验拒绝(报 `INVALID_PATH`)。**这正说明校验在按预期工作**,改为 `xxx_ex.nodb`。
- **明确暂不做**(理由记于此,避免反复纠结):macOS 代码签名/notarization 需要开发者账号与证书,在无真实下游前是纯成本;npm/go module 分发元数据同理。预编译包 + SHA256 已覆盖自用与早期采用者。
- 合计 **354 例全绿**,clippy 零警告;Python ctypes 示例回归通过。

### 43. v0.3.0 M20 设计:阶段 2 —— `nosqlite-server`(MongoDB wire 协议)

- 目标:用 **Go 官方驱动** `go.mongodb.org/mongo-driver/v2` 连上,让 `nosqlite-server` 可替代 mongodb-server。
- 第一步目标(本里程碑):**驱动能连上 + 核心 CRUD 跑通**,不是全协议。
- 协议事实(决定实现顺序):
  - 传输:TCP,消息 = 标准头部(16 字节,含 `messageLength`/`requestID`/`responseTo`/`opCode`)+ 体;
  - 现代驱动只用 **OP_MSG**(opCode 2013),`section 0` 为单文档 BSON;带 `$db`、`$readPreference` 等;
  - 握手:驱动首发的命令是 **`hello`**(老驱动可能是 `isMaster`/`ismaster`),响应必须含 `maxWireVersion`;
  - **`maxWireVersion` 给低了驱动会直接拒连**:以驱动 v2.5+ 要求为准,先给 **17**;
  - 驱动关闭连接时会发 `endSessions`,**必须能回应**(空 ok),否则挂起。
- 必须实现的命令(按此顺序落地):
  1. `hello` / `isMaster`、`ping`
  2. `insert`、`find`、`update`、`delete`
  3. `findAndModify`(Mongoose `findOneAndUpdate` 的底层)
  4. `count`、`distinct`、`listCollections`、`listDatabases`、`createIndexes`、`drop`、`dropDatabase`
  5. `getMore` / `killCursors`(游标必需)
  6. `aggregate`(可先只支持 `$match/$sort/$limit/$skip`,其余明确报错)
- **BSON 编解码是独立难题**:需自行实现(不引 crate 以保持依赖精简?→ 评估后定,优先正确性)。
  - 需支持类型:double、string、document、array、binary、ObjectId(12 字节)、bool、UTC datetime、null、int32、int64、timestamp、minkey/maxkey;
  - 与内部 JSON 的映射借助 M14/M17 的 **Extended JSON** 契约:
    `ObjectId` ↔ `{"$oid": "..."}`、`Date` ↔ `{"$date": ...}`、`int64` ↔ `{"$numberLong": "..."}`;
  - `_id` 在协议层必须是 **BSON ObjectId**,存储里是 24 位 hex 字符串 —— **翻译在 server 层完成**,存储不变。
- 架构:`nosqlite-server` 为**同一个 crate 的 bin**(`src/bin/nosqlite-server.rs`),复用 `Database`,不另建 crate,避免跨 crate 重复依赖。
- 进程模型:**单进程独占 `.nodb`**,因此 redb 的单写者约束天然满足;连接多线程共享一个 `Database`(`Send + Sync` 已保证)。
- 不做(本里程碑):认证(先不做/或做最简 SCRAM 跳过)、副本集、分片、事务会话、`changeStream`、压缩。

### 44. v0.3.0 M20 执行:阶段 2 —— nosqlite-server(MongoDB wire 协议)

- **结果:Go 官方驱动 `go.mongodb.org/mongo-driver/v2` 全部 9 项验收通过**(Ping/插入查询/批量+游标/更新/删除/findAndModify/upsert/Date/主键冲突)。
- 新增模块:
  - `src/server/bson.rs` —— BSON ↔ JSON 编解码,零新依赖。类型映射见模块文档;`ObjectId` ↔ 24 位 hex 字符串、`datetime` ↔ `{"$date": ms}`、`int64`/`int32`、binary/regex/decimal128/minKey/maxKey 等。
  - `src/server/wire.rs` —— OP_MSG/OP_QUERY 帧编解码;`Request` 含 `command` + `sequences`。
  - `src/server/serve.rs` —— TCP 监听(每连接一线程)+ 命令分发(模块名由 `server` 改为 `serve`,否则 `server::server` 触发 clippy `module_inception`)。
  - `src/bin/nosqlite-server.rs` —— 独立入口,`--addr` 默认 `127.0.0.1:27017`。
- **三个关键坑(都是真实 driver 打出来的,不实测根本发现不了)**:
  1. **OP_MSG section 1**:驱动把批量文档放在 section 1(带 identifier:`documents`/`updates`/`deletes`),命令文档里**没有**这些字段。只读 section 0 表现为「插入 0 条 / 更新 0 条」。`Request.sequences` 按 identifier 分组解决了这一点。
  2. **`cursor.id` 必须是 BSON int64**:用普通数字 `0` 会被编码成 int32,驱动报 `id should be an int64 but it is a BSON 32-bit integer` 并拒绝整个响应。必须写 `{"$numberLong": "0"}` 触发 int64 编码。
  3. **OP_REPLY 头部漏算 4 字节**:旧驱动握手走 OP_QUERY/OP_REPLY,`numberReturned` 字段前还有个 `startingFrom`;长度算错会得到 `number of documents returned does not match numberReturned`。
- **集合名取法**:MongoDB 约定集合名在**命令名对应的键值**里(`{insert: "users"}`),不是 `collection` 字段。初版搞错导致全部报「集合名不能为空」。
- **`countDocuments` 走聚合**:底层是 `aggregate` + `{$group: {_id: null, n: {$sum: 1}}}`,不实现 `$group` 就无法 count。已实现该形态(以及 `$project/$addFields` 的简化版)。
- **已实现命令**:`hello/isMaster`、`ping`、`endSessions`、`killCursors`、`insert`、`find`、`update`、`delete`、`findAndModify`、`count`、`distinct`、`listCollections`、`listDatabases`、`createIndexes`(接受不报错)、`drop`、`dropDatabase`、`buildInfo`、`aggregate`(`$match/$sort/$skip/$limit/$group/$project`)。
- 测试:
  - `tests/mongo_wire.rs` 2 例 —— 自动拉起服务 + **用 Go 官方驱动跑 9 项**;另有不依赖 Go 的裸 `hello` OP_MSG 握手校验(防协议字段被误删)。无 Go 工具链时自动跳过。
  - `src/server/bson.rs` 8 例单测(标量/嵌套/`_id`→objectId/Date/ISO 输入/int64/空容器/损坏输入)。
  - 合计 **367 例全绿**,clippy 零警告。
- 未做(留待后续):认证、副本集、分片、changeStream、真实游标(`getMore` 分批,当前一次性返回并置 `id=0`)、完整 `$group` 累加器、索引。

### 45. v0.3.1 M21 设计:真实游标(`getMore` / `killCursors`)

- 现状缺陷:阶段 2 的 `find` 一次性返回全部结果并把 `cursor.id` 置 0。后果:
  - 大结果集会**一次性全部读进内存**(与 M11 花大力气消除的 O(集合大小) 目标相悖);
  - 驱动的 `batchSize` 只能靠 `limit` 模拟,语义不准;
  - `getMore`/`killCursors` 是空实现,驱动真发 `getMore` 时会拿到空结果。
- 设计:
  - **服务端维护游标注册表**:`HashMap<i64, CursorState>`,进程级共享(`Mutex`),游标 ID 为自增 i64(必须 > 0,0 表示"已耗尽")。
  - `CursorState`:集合名 + 剩余文档列表 + 已读偏移 + 投影/排序参数。
  - **简化但仍正确的实现**:`find` 时完成过滤+排序+投影,把**结果集**存在游标里,按 `batchSize` 分批返回。这样:
    - 驱动侧语义正确(`getMore` 能继续取、`batchSize` 生效、`id` 非 0);
    - 内存上限是「本次查询结果集」,而非整个集合 —— 比现状严格更好;
    - 不触碰 `Storage` 抽象。
  - 真正的「边扫边吐」需要可持有的迭代器(会动 `Storage`),留到有明确内存压力时再做。
  - `getMore: {getMore: <id>, collection, batchSize}` → 返回下一批;耗尽时 `id: 0` 并移除游标。
  - `killCursors: {killCursors: "<coll>", cursors: [<id>...]}` → 删除并回 `cursorsKilled/cursorsNotFound`。
  - 游标在连接断开时不强制清理(简化),但提供上限保护:单进程游标数超阈值(如 1000)时拒绝新游标,避免泄漏打爆内存。
- `aggregate` 同样接入游标(输出管道结果)。
- 验收:Go 驱动 `SetBatchSize` 分页遍历、`getMore` 多次调用、结果数量与内容正确。

### 46. v0.3.1 M21 执行:真实游标(`getMore` / `killCursors`)

- **游标注册表**:`CursorRegistry`(进程级 `OnceLock`),`Mutex<HashMap<i64, CursorState>>` + `AtomicI64` 自增 ID(始终 > 0);上限 1000 个,超限返回 `InternalError` 而非默默泄漏。
- **`find` 语义修正**:
  - `limit` 现在只约束**结果总数**,不再被 `batchSize` 夹取;
  - `batchSize` 只决定**首批条数**(缺省 101,与 MongoDB 一致);
  - 结果集超过一批时注册游标并返回非 0 ID,否则 `id = 0`(不注册)。
  - 顺带修掉阶段 2 的语义错误:之前把 `min(limit, batchSize)` 当作 limit,导致 `limit=12 & batchSize=5` 只返回 5 条。已由 Go 测试 `TestLimitWithBatchSize` 锁定。
- **`getMore`**:按 `batchSize` 续取;取完最后一批时注册表移除游标并回 `id = 0`;游标不存在回 `CursorNotFound(43)`。
- **`killCursors`**:逐个删除,回 `cursorsKilled` / `cursorsNotFound`(此前是空实现,驱动提前关游标时会拿不到反馈)。
- **`aggregate` 接入游标**:支持 `cursor.batchSize`,大管道不再一次性全部返回。
- **内存取舍(写进代码注释)**:游标保存的是**已过滤/排序/投影后的结果集**,不是底层迭代器。内存上限从「整个集合」降为「本次查询结果集」,比阶段 2 严格更好;真正的边扫边吐需要可持有的 `Storage` 迭代器,留到有明确内存压力时再做。
- 测试:
  - Go 驱动 +4 例:`TestCursorBatching`(120 条 + `batchSize=7`,强制多次 `getMore` 且校验顺序连续)、`TestCursorEarlyClose`(读 3 条即关,触发 `killCursors`)、`TestAggregateCursor`、`TestLimitWithBatchSize`。
  - Rust +5 例游标单测(分批耗尽、kill、单批内 id=0、超批 id≠0、`batchSize=0` 钳制)。
  - 合计 **372 例全绿**,Go 端 13 项全通过,clippy 零警告。

### 47. v0.4.0 M22 设计:ScanSession(活游标)+ 索引总方案

- **定位澄清**(见「双形态路线」):部署形态学 SQLite,数据模型/索引/游标语义 100% 学 MongoDB。
- **M22:ScanSession** —— 引擎内部真流式,是 SQLite `sqlite3_stmt` 的等价物。
  - 现状缺陷:`Storage::for_each` 是回调式,回调结束读事务即关闭 → 游标只能缓存整个结果集;索引扫描(索引取 id → 回表)也无法跨调用进行。
  - 新增 `Storage::scan_session(collection) -> Box<dyn ScanSession + '_>`;`ScanSession::next_batch(max)` **从当前迭代器位置继续**,不重扫。
  - **不返回 `Iterator` 而返回 `next_batch(max)`**:C ABI 拿不了 Rust 迭代器;`next_batch` 能直接映射成 C 游标与 wire 的 `getMore`;也避免 redb guard 的生命周期体操。
  - `for_each` 保留(内部改用 scan_session 实现),确保 372 例测试不破。
  - 对外接口本里程碑**不暴露**,纯内部重构,测试全绿后再作为后续出口。
- **索引总方案(M23–M25,本次只定框架不实现)**:
  - 索引表 `(集合名\0字段路径\0编码值\0doc_id) -> ()`,独立 B+树,与 MongoDB 的「独立索引」结构一致。
  - **类型序编码**:把 MongoDB/BSON 类型序落成字节序 —— 缺失 < null < number < string < object/array < bool < date。**这是我们比较语义(M13)的直接投影,不是自创**;索引序与查询比较序必须完全一致,否则会静默漏数据。
  - **multikey 从第一天进模型**:索引条目结构即为 `(集合, 字段路径, 编码值, doc_id)`;数组字段展开为多条条目只是「一次插入写 N 条」,存储格式不变。M23 可先只处理标量,M24 加查询时不改格式。
  - **编码版本化**:索引表内记录 `encoding_version`;未来升级编码时检测旧版本自动重建(重建期间回退全表扫,数据不丢)。现在做成本为 0,以后补要处理存量。
  - 游标必须有**超时自动清理**(对齐 MongoDB `cursorTimeoutMillis` 默认 10 分钟),防止客户端崩溃后游标占内存至上限。

### 48. v0.4.0 M22 执行:ScanSession(活游标)

- **`Storage::scan_session(collection) -> Box<dyn ScanSession>`**:会话持有 redb 读事务与 B+树迭代器,`next_batch(max)` **从上次位置继续**,不重扫。
- **关键设计决策(被编译器逼出来的,值得记住)**:会话一开始设计成 `Box<dyn ScanSession + '_>` 借用 `&Database` → 借用检查直接否决:用户持有游标时无法再移动/释放 `Database`,可用性极差。改为**会话自持有 `redb::ReadTransaction`**(redb 的 `Range<'static>` 本就自持读事务),`ScanCursor` 因此**没有生命周期参数**。这是"现在就该设计"的最好例证 —— 接口形态一旦定错,后面索引扫描和 C ABI 出口都要跟着返工。
- `for_each` 内部改为基于 `scan_session` 实现(每批 1024 条):消除两份几乎相同的遍历代码,且**回调语义完全不变**,372 例既有测试零修改通过。
- 新增 `Database::scan()` → `ScanCursor { next_batch / next_docs }`,**纯内部能力,暂不对外暴露**(C ABI 与 wire 层留到 M23 之后接入)。
- `RedbScanSession` 为 `empty()` 场景(表不存在)保留读事务,行为一致。
- 测试:新增 `tests/scan_session.rs` **10 例** —— 分批恰好覆盖全量无重复遗漏、`next_docs` 与 `find` 结果一致、耗尽幂等、空集合、`max=0` 不推进、集合隔离、**会话快照隔离(MVCC:会话中途插入不影响已开会话,新会话能看到)**、500 条小批次、主键有序、大文档不截断。合计 **382 例全绿**,clippy 零警告。

### 49. v0.4.1 M23a 设计:类型序修正为 BSON 官方序(破坏性)

- **背景**:M23 准备做索引时核对发现,我们的 `type_rank` 与 MongoDB 的 BSON 比较序**不一致**。
  这不是「索引怎么做」的问题,而是**查询语义本身有偏差**;索引只是把它放大:
  若按错误的序建索引,跨类型范围查询会返回错误结果。
- **MongoDB 官方 BSON 比较序**(`bson-type-order`):
  `MinKey < Null < Number < Symbol/String < Object < Array < BinData < ObjectId < Boolean < Date < Timestamp < Regex < MaxKey`
- **现状对照**:

  | 类型 | MongoDB | 现状(v0.4.0) |
  |---|---|---|
  | null | 1 | 1 ✅ |
  | number | 2 | 3 ❌ |
  | string | 3 | 5 ❌ |
  | object | 4 | 7 ❌ |
  | array | 5 | 6 ❌ |
  | bool | 8 | 2 ❌ |
  | date | 9 | 4 ❌ |

  除 null 外**全部错位**。
- **决策**(用户确认):
  1. 修正为 BSON 官方序(**破坏性变更**,排序与跨类型比较结果会变);
  2. 一并补上 `MinKey`/`MaxKey`/`Timestamp` 类型位置(driver 会发 `{$gt: MinKey}` 这类查询)。
- **实现要点**:
  - 内部等级号直接对齐 BSON 序,便于将来写索引用同一套常量;
  - `Timestamp` 比较:`(t, i)` 元组比较,与 BSON 语义一致;
  - `BinData`/`ObjectId`/`Regex`/`Decimal128`:目前以 Extended JSON 表示,按其 BSON 等级排位;
  - **同一真相源**:索引编码(下一步)必须复用这套等级定义,并用测试锁死「`compare_values` 结果 == 编码字节比较结果」。
- **影响面**:同类型内部比较(数字/字符串/布尔)不变,仅**跨类型**排序与比较改变,实际风险小于听起来。
- **待定(下一步 M23b)**:索引编码采用 order-preserving(类型前缀 + 大端/IEEE 翻转),因为它能直接支撑 range 扫描;
  不复用 BSON 字节直接做 key —— BSON 的 int32 小端序会使字节序 ≠ 值序。

### 50. v0.4.1 M23a 执行:类型序修正为 BSON 官方序

- `query/compare.rs` 的 `type_rank` 重写,等级直接对齐 MongoDB 官方序,并抽成**公开的 `rank` 模块**作为单一真相源:
  `MIN_KEY(0) < NULL(1) < NUMBER(2) < STRING(3) < OBJECT(4) < ARRAY(5) < BINARY(6) < OBJECT_ID(7) < BOOL(8) < DATE(9) < TIMESTAMP(10) < REGEX(11) < MAX_KEY(12)`。
- 新增类型支持:
  - `MinKey`/`MaxKey`(`{"$minKey":1}`/`{"$maxKey":1}`)—— 恒为端点,支撑 `{$gt: MinKey}` 全量查询;
  - `Timestamp`(`{"$timestamp":{"t":..,"i":..}}`)—— 按 `(t, i)` 元组比较;
  - `Regex`/`Binary` —— 按 BSON 等级排位(原先会被误判为 OBJECT)。
- **缺失字段改为按 `null` 等级**(与 MongoDB 一致;旧实现把缺失当成"最小值",排在 null 之前)。
- `compare_values` 重写:先比等级,同等级再按类型分发,特殊字面量优先于 `normalize` 判定。
- 导出 `query::{type_rank, as_timestamp}` 与 `query::compare::rank` 常量。
- **破坏性变更**(已确认):跨类型排序与比较结果改变。同类型内部比较**完全不变**(数字/字符串/布尔/日期)。
- 测试:
  - 新增 `tests/bson_type_order.rs` **10 例** —— 12 种类型相邻两两双向校验、缺失≡null、同类型比较不变、Timestamp 元组序、MinKey/MaxKey 端点、跨类型范围查询、混合类型排序完整序列、等级常量锁定、`$oid`/`$numberLong` 归一后的等级。
  - 修正 3 处固化了旧序的既有用例:`query::compare::tests::type_order`、`tests/compare_composite.rs::type_order_still_holds`、`tests/date.rs::date_type_rank`。
  - 合计 **392 例全绿**;Go 驱动验收(M22 后)仍全通过,**wire 层未受影响**。

### 51. v0.5.0 M23b 设计:索引框架(order-preserving 编码 + 独立索引表)

- **前提**:M23a 已把 `type_rank` 对齐 MongoDB 官方 BSON 序,并抽成 `query::compare::rank` 常量作为单一真相源。
- **编码方案(用户已确认不复用 BSON 字节)**:order-preserving encoding。
  - 不复用 BSON 直接当 key 的原因:BSON 的 int32/int64 是**小端序**,字节序 ≠ 数值序(负数尤其错乱),无法直接做 range 扫描。
  - 编码规则(与 `rank` 常量严格对应):
    | 类型 | 首字节 | 后续 |
    |---|---|---|
    | MinKey | `0x00` | 无 |
    | Null | `0x01` | 无 |
    | Number | `0x02` | **IEEE754 位翻转**:负数整体取反,正数翻最高位 → 大端字节序即数值序 |
    | String | `0x03` | UTF-8 原字节(字典序正确) |
    | Object | `0x04` | 键值对序列 |
    | Array | `0x05` | 元素序列 |
    | Binary | `0x06` | 子类型 + 数据 |
    | ObjectId | `0x07` | 12 字节 |
    | Bool | `0x08` | 0/1 |
    | Date | `0x09` | i64 大端(偏移 +2^63 使负数可排) |
    | Timestamp | `0x0A` | (t,i) 大端 |
    | Regex | `0x0B` | pattern + options |
    | MaxKey | `0x0C` | 无 |
- **索引表布局**:`(集合名\0 字段路径\0 编码值\0 doc_id) -> ()`,独立 B+树(`nosqlite_indexes` 存定义、`nosqlite_index_data` 存条目)。
  - doc_id 拼在末尾(学 SQLite 的 rowid 拼接):保证 key 唯一,且相同索引值下天然按 doc_id 有序。
  - **multikey 进模型**:数组字段展开为多条条目(同一 doc 写 N 条),存储格式不变;本期先只处理标量,数组按 multikey 展开一起做。
- **索引定义表** `nosqlite_index_meta`:`(集合\0索引名) -> {字段, 唯一, sparse, encoding_version}`。
  - **`encoding_version`**:编码一旦落盘,升级编码需重建索引。现在存版本号成本为 0,以后补要处理存量数据。
- **写路径维护**:`insert/insert_many/update/update_many/delete/delete_many` 在**同一写事务**内同步更新索引。
  更新时必须**先取旧文档算出旧索引键 → 删旧条目 → 插新条目**,否则留垃圾。
  - 这一步要求 `redb_engine` 的写路径能读到旧值(现有 `update` 已用 `insert` 返回旧值,但索引需要旧文档内容,故需在事务内先 `get` 再写)。
- **验收铁律**:测试必须锁死「**对任意两个值,`compare_values` 的结果 == 编码字节的比较结果**」。
  这是整个索引正确性的根基:两者不一致,范围查询走索引就会静默返回错误结果。
- 本期**不改查询路径**(不实现查询规划器),只建索引与维护;规划器留到 M24。

### 52. v0.5.0 M23b 执行:索引框架(编码 + 独立索引表 + 写路径维护)

- **`src/index/key.rs` —— order-preserving 编码**:
  - 首字节 = [`rank`] 等级(**直接复用 M23a 的单一真相源**,保证索引序与比较序不分叉);
  - Number 用 **IEEE754 位翻转**(负数整体取反、正数翻最高位)+ 大端 —— 这是 BSON 小端字节做不到的关键;
  - Date 用 `ms ^ 2^63` 偏移后大端;Timestamp `(t,i)` 各 32 位大端;
  - 数组/对象用**长度前缀**拼接子编码,避免歧义;数组末尾补 `0` 使短数组排在前(与 `compare_values` 一致)。
- **`src/index/def.rs` —— 定义与条目 key**:
  - 条目 key `集合名\0字段路径\0[编码值长度][编码值]doc_id`:编码值可能含 `0x00`,故用**长度前缀**而非分隔符;
  - `doc_id` 拼在末尾保证唯一 + 同值条目按 doc_id 有序;
  - `entry_values` 实现 **multikey**(数组每元素一条)、**稀疏**(null/缺失不入)、嵌套路径。
- **存储表**:`nosqlite_index_meta`(定义)、`nosqlite_index_data`(条目);`INDEX_ENCODING_VERSION = 1`。
- **写路径维护**:`insert/insert_many/update/update_many/delete/delete_many` 全部在**同一写事务**内维护索引。
  - **update/delete 必须先 `get` 旧文档**才能算出旧索引条目;更新顺序严格为「删旧条目 → 写新条目」。
  - 唯一索引在 `write_index_entries` 内做**同值前缀扫描**,排除自身 doc_id 后若还有其他文档 → `UniqueViolation`。
- **新错误变体** `StorageError::UniqueViolation { collection, index }`,映射到 `ErrorCode::DuplicateKey`(wire 层即 `11000 DuplicateKey`)。
- **暴露接口**:`Database::{create_index, list_indexes, drop_index}`。`create_index` 幂等(同名先清旧条目再回填),回填在单事务内,失败整体回滚。
- **测试**:
  - `tests/index_encoding.rs` **11 例** —— 核心是 **`encoding_order_matches_comparison`:36 个样本两两穷举,断言 `compare_values(a,b) == encode_key(a).cmp(encode_key(b))`**。这条测试立刻抓到一个真实分叉:`$oid` 在比较层归一为字符串、在编码层却按 OBJECT_ID 排位。已统一为「`$oid` ≡ 字符串」(与存储层主键即 hex 字符串的契约一致)。
  - `tests/index_basic.rs` **18 例** —— 定义增删/幂等、唯一索引拒绝既有与新增重复、同文档自更新不冲突、稀疏跳过 null、**更新移动索引条目(旧值释放)**、删除清理条目、批量删改维护、multikey(数组元素/自重复/删除清理)、回填、持久化重开、嵌套路径、无索引回归。
  - `src/index/def.rs` 6 例单测(0x00 编码值可无歧义解析、JSON 往返、multikey、稀疏、嵌套路径、重复检测)。
  - 合计 **427 例全绿**;Go 驱动验收仍全通过(clippy 零警告)。

### 53. v0.5.1 M24 设计:查询规划器(让索引真正被用上)

- 现状:M23b 已把索引建好、写路径维护好,但**查询仍走全表扫** —— 索引只对唯一约束生效。
- 目标:filter 里的等值/范围条件能匹配到索引 → 走 `IXSCAN` → 回表;`explain` 可观测。
- 设计:
  - **候选索引匹配**:遍历集合的索引定义,若 filter 顶层存在「该索引字段」的单条件(等值或范围),则可用。
    - 优先级:等值 > 范围(等值能用精确 key,范围用区间扫描)。
    - 复合索引本期不做(单字段索引先行)。
  - **索引扫描**:用编码后的区间在 `nosqlite_index_data` 上 `range(start..end)`,取出 doc_id 列表(去重),再按主键回表读文档。
    - 区间构造:`$eq` → `[enc(v), enc(v)+0xFF)`;`$gt/$gte/$lt/$lte` 按编码字节上界/下界构造(编码是 order-preserving,所以字节比较即值比较)。
  - **回表**:用含 `filter.matches()` 的二次过滤 —— 索引只负责缩小候选集,**正确性仍由完整 filter 保证**(这是关键:即使索引区间构造有偏差,也不会返回错误结果,只会多读)。
  - **排序**:若 `sort` 只有一个字段且该字段正有可用索引,按索引顺序返回,免掉 `find_page` 的两遍法。
  - **`explain`**:`Database::explain(collection, filter, sort)` 返回计划描述
    `{stage: "IXSCAN"/"COLLSCAN", indexName, indexBounds, docsExaminedGuess}`;wire 层暴露 `explain` 命令供驱动调用。
- **安全底线**:索引扫描结果必须再走一遍 `filter.matches`,不与全表扫结果产生分歧。测试用「同一 filter 两条路径结果必须相同」锁死(复用 M23b 的两两穷举思路)。
- 不做(留 M25):复合索引、交集/并集计划、代价估算、`$or` 走索引。

### 54. v0.5.1 M24 执行:查询规划器

- **`src/index/planner.rs`**:`plan_with_json(filter_json, sort, indexes) -> Plan`。
  - `Plan::CollScan` / `Plan::IndexScan { index, bounds, sorted_by_index }`;
  - 区间构造:等值 `[enc, enc+0xFF)`、`$gte` `[enc, ∞)`、`$gt` `[enc+0x00, ∞)`、`$lte` `(-∞, enc+0xFF)`、`$lt` `(-∞, enc)`;
  - `sort` 单字段与索引字段一致 → `sorted_by_index = true`(免掉两遍法)。
- **`Database::find_planned(collection, filter, filter_json, options)`**:索引区间扫描 → 回表 → **完整 `filter.matches()` 二次过滤** → 排序/分页/投影。无可用索引时退化为 `find_with_options`。
- **`Database::explain(collection, filter_json, sort)`**:返回 `{stage, indexName, field, bounds, sortedByIndex, filterApplied}`。
- **安全底线**:索引只缩小候选集,**正确性由完整 filter 保证**;`explain` 里显式输出 `filterApplied: true` 提醒调用方。
- **`SortSpec::single_field_path()`** 新增(规划器判断能否利用索引序)。

### 三个被真实测试打出来的缺陷(都已修)

1. **索引 key 的长度前缀破坏值序**(最严重,M23b 遗留):
   - 原布局 `prefix + [len] + enc + doc`,B+树按字节比较 → 「len=2 的 bool」排在「len=9 的 number」之前,与 `compare_values` 相反;
   - 后果:`{"n": {"$gte": 2}}` 走索引**漏掉 bool 文档**(BSON 序里 bool(8) > number(2));
   - 修复:长度改为**后缀** `prefix + enc + [len] + doc`,排序只由 enc 决定;
   - 解析侧新增 `split_entry`(遍历候选切分点,校验长度自洽)与唯一检查用的精确切分。
2. **写事务内开读事务**(redb 约束):`insert/update/delete` 系列原在 `begin_write()` **之后**调 `list_indexes()`,后者内部 `begin_read()`。已全部把索引定义读取提到写事务**之前**。
3. **唯一性检查的 key 解析未跟上新布局**:改为 `probe = prefix + enc`,再校验长度后缀等于 `enc` 长度,避免「更长值以该编码为前缀」被误判。

- 测试:新增 `tests/index_planner.rs` **12 例**,核心是 **`index_results_match_collscan`**:同一批 filter 分别在「不建索引」与「建索引」两条路径跑,断言 `_id` 集合完全一致(覆盖等值/范围/跨类型/无命中/非索引字段)。另有排序分页一致性、multikey、更新/删除后索引同步、explain 标记等。
  - 合计 **440 例全绿**;Go 驱动验收仍全通过;clippy 零警告。

### 55. v0.5.2 M25 设计:复合索引 + wire 层 explain

- 现状:M24 的单字段索引 + 规划器已可用,但有两个明显缺口。
- **缺口一:复合索引**。CMS 里最常见的查询是 `{tenant: "t1", status: "ok"}` 或
  `{tenant: "t1", createdAt: {$gte: ...}}`,单字段索引只能命中其中一个条件。
- **缺口二:wire 层没有 `explain`**。驱动/`mongosh` 无法观测计划,M24 的能力在服务端不可见。

- 设计:

| 项 | 方案 |
|---|---|
| 复合索引编码 | `IndexDef.fields: Vec<String>`,条目 key = `集合\0字段路径串\0编码值1 编码值2 ... 后缀长度 doc_id`。**前缀原则**:`{a:1,b:1}` 可服务 `{a:...}` 查询,反之不行 |
| 规划器 | 按「前缀匹配」打分:等值字段越多、越靠前,优先级越高;范围字段只能出现在最后一个等值字段之后 |
| multikey 复合 | 任一字段是数组即展开(笛卡尔积上限保护,超过则不入索引) |
| wire explain | 新增 `explain` 命令,返回 `{queryPlanner: {...}}`,形态贴近 MongoDB |
| 兼容性 | `IndexDef` 增加 `fields` 字段,同时保留单字段 path;旧索引数据(单字段)仍可读 —— `encoding_version` 由 1 升到 2 触发重建 |

- **关键约束**:复合索引的条目 key 必须保持 order-preserving —— 每个字段的编码值**依次拼接**,
  由于 `encode_key` 的输出自定界(首字节类型 + 长度后缀在整体末尾),拼接后的字节序仍等价于
  「按字段顺序逐级比较」,这正是 MongoDB 复合索引的顺序语义。
- 验收:沿用 M24 的铁律 —— **索引路径与全表扫结果必须完全一致**,复合索引亦然。

### 56. v0.5.2 M25 执行:复合索引 + wire 层 explain

- **`IndexDef` 改造**:`field: String` → `fields: Vec<String>`(保留 `field()` 访问器做兼容);新增 `new_composite`/`is_composite`/`field_signature`(字段用 `,` 连接,唯一标识索引的字段与顺序)。
  - JSON 序列化改为 `fields`,**读取时兼容旧的 `field` 单字段格式**。
- **条目 key 布局(复合)**:`集合\0字段签名\0[enc1..encN][len1..lenN]doc_id`。
  - 各字段编码值**依次拼接** —— 因 `encode_key` 输出自定界(首字节即类型),拼接后的字节序恰好等价于「按字段顺序逐级比较」,正是 MongoDB 复合索引的顺序语义;
  - **所有长度后缀统一放在编码值之后**(M24 教训的推广),确保排序只由编码值决定;
  - `split_entry_multi(rest, segments)` 用「遍历候选总长 + 校验各段长度之和」定位切分。
- **multikey 复合**:任一字段是数组即参与笛卡尔积展开,上限 128 组(超限整体不入索引),防止字段组合爆炸。
- **规划器(前缀原则)**:索引 `{a,b}` 可服务 `{a}` 与 `{a,b}`,不能服务 `{b}`;范围条件只能出现在最后一个等值前缀字段之后。多候选时按「前缀字段数 > 是否全等值」择优。
- **公开 API**:`Database::create_index_composite(collection, name, fields, unique, sparse)`。
- **wire 层 `explain`**:`{explain: {find: "coll", filter: {...}, sort: {...}}}` → `{queryPlanner: {namespace, winningPlan: {stage, indexName, bounds, sortedByIndex, filterApplied}}, ok: 1}`。
- **过程中修的结构性问题**:多轮机械替换把 `redb_engine.rs` 弄出重复块(两处 `impl RedbEngine`、重复的 `split_entry`/`write_index_entries`),靠 `grep '^impl'`/行号定位删除修复;`split_entry_multi` 从引擎层**移入 `index/def.rs`**(它只依赖 key 布局,与 redb 无关)。
- 测试:
  - `tests/index_composite.rs` **8 例** —— 前缀可用/后缀不可用、复合+范围、**`composite_results_match_collscan`(多条件组合与全表扫逐一对比)**、复合唯一、复合 multikey、更新同步、单字段回归;
  - `src/index/def.rs` 3 例(key 含 0x00、长度后缀不破坏值序、**复合 key 逐级排序**);
  - Go 驱动 +1 例(`TestExplain`,用递归查找兼容 BSON 嵌套类型的两种表示)。
  - 合计 **449 例全绿**,Go 13+1 项全通过,clippy 零警告。

### 57. v0.5.3 M26 设计:$or 走索引 + 索引交集计划 + wire createIndexes

- 背景:M25 交付了复合索引,但规划器仍只认「顶层字段条件的等值前缀」,并且 wire 层 `createIndexes` 只回 `ok`——**Go 驱动建不了索引**,这是实际缺口。
- 范围(用户确认三项):
  - **① `$or` 走索引**:`{$or: [{a:1},{a:2}]}` 拆成多个索引区间取并集(union plan);
  - **② 索引交集(intersection)**:`{a:1, b:2}` 且分别有 `a`、`b` 两个索引时,取候选 doc_id 交集,再回表;
  - **③ wire `createIndexes` 真正建索引**,并补 `listIndexes` / `dropIndexes`。
- 设计要点:
  - **并集/交集都只缩小候选集**,回表后一律再跑完整 `filter.matches()`——沿用既有铁律;
  - **交集判定用 doc_id 集合**,不做「索引下推排序」,复杂度 `O(n log n)`,候选集通常远小于集合;
  - **`$or` 的 union 仅在每个分支都能走索引时启用**,任一分支走不了就退回全表扫(避免「部分索引 + 部分扫描」的复杂度);
  - **wire `createIndexes`** 解析 `{createIndexes: coll, indexes: [{key: {a:1,b:-1}, name, unique, sparse}]}`,返回真实 `numIndexesBefore/After`;
  - **`drop_collection` 顺带清理该集合的索引定义与条目**(M25 遗留待补项)。
- 明确不做:索引统计/区分度估算(需采样,收益不确定)、TTL 索引。

### 58. v0.5.3 M26 执行:$or 走索引 + 索引交集 + wire createIndexes

- **计划模型扩展**:`Plan` 新增 `Union { branches }`(`$or` 并集)与 `Intersect { branches }`(多索引交集);`stage()` 分别返回 `OR` / `AND`;`is_index_scan()` 改为 `!matches!(self, CollScan)`。
- **`$or` 并集**:拆各分支单独规划,**全部分支都是索引计划**时才整体走索引,任一分支走不了就退回全表扫(不做「部分索引 + 部分扫描」的混合执行)。并集按出现顺序去重。
- **索引交集**:顶层字段条件各自找以该字段开头的索引,分支数 >= 2 时取交集;执行时**以候选最少的分支为基准**再逐个校验其余集合。
- **两个真实缺陷(写测试时暴露)**:
  - **无限递归 → 栈溢出**:交集分支内部调 `plan_conjunction` 会再次尝试构造交集,形成 `conjunction → intersection → conjunction → …`。修法:`plan_conjunction_inner(..., allow_intersection: bool)`,分支内传 `false`。**教训:计划构造这类「生成器互相调用」的结构必须显式传递「禁止再生成」的开关**。
  - **复合索引被交集抢占**:`{a,b}` 复合索引完整覆盖 filter 时,原本会退化成「a 索引 ∩ b 索引」两次扫描。修法:`primary` 是覆盖全部 filter 字段的复合索引时直接返回 `None`,不构造交集。
- **`drop_collection` 补清索引**(M25 遗留):同事务内按 `集合名\0` 前缀清空索引定义表与索引数据表。
- **wire 层三个命令真正落地**:
  - `createIndexes`:解析 `{indexes: [{key, name, unique, sparse}]}`,**`key` 的键序即索引字段顺序**,返回真实 `numIndexesBefore/After`;未给 `name` 时按 MongoDB 惯例拼 `字段_方向`;
  - `listIndexes`:返回游标形态(`cursor.firstBatch`),含虚拟主键索引 `_id_`;
  - `dropIndexes`:`*` 全删或按名字删。
- **BSON 编码补 `{"$numberLong": "..."}`**:游标 `id` 驱动要求 **int64**,而现有编码器对整数优先写 int32,导致驱动报「id should be an int64 but it is a BSON 32-bit integer」。用扩展 JSON 标记强制 64 位。
- 测试:`tests/index_union_intersect.rs` **8 例**(OR 并集/去重/范围分支/分支不可索引时回退、交集/大量条件逐一对比/单索引不生成 AND、删集合清索引);Go 驱动 **+1 例**(`TestCompositeIndexOverWire`),原 `TestExplain` 升级为「建索引 → IXSCAN → listIndexes → dropIndexes → COLLSCAN」完整闭环。合计 **457 例全绿**,Go 15 项全通过,clippy 零警告。

### 59. v0.5.4 M27 设计:降序索引(修正 `-1` 被静默当升序)

- **缺陷定位(M26 遗留)**:wire 层 `createIndexes` 收到 `{a: -1}` 时,把 `-1` 丢掉按升序建索引,且 `IndexDef` 根本不存方向。后果有两个,都是**正确性缺口**:
  1. `explain` 会把「`sort {a: -1}` + 索引 `a: -1`」判定为 `sortedByIndex`(可省排序),但实际索引是升序 → **返回顺序错**;
  2. 用户拿不到语义承诺的索引遍历方向。
- 设计:
  - `IndexDef` 增加 `directions: Vec<i8>`,与 `fields` **一一对应**(`1` 升序 / `-1` 降序);
  - **条目 key 编码不变**:降序索引仍按**升序**存,区别只在**扫描方向**。这是关键决策 —— 若按降序编码(取反字节),范围区间的语义、multikey 展开、唯一性检查全都要跟着改,风险远大于收益;而「逆序遍历升序索引」在 B+树上等价于降序索引,只是区间端点要互换:
    - 升序扫描:`[lower, upper)`,正序产出;
    - 降序扫描:`(upper, lower]` 反向产出 —— 实现上仍用 `[lower, upper)` 取到候选,再**反转**结果顺序;
  - `IndexDef::directions` 缺省为全 `1`(兼容 M26 及更早的索引定义,`from_json` 里 `directions` 缺失时补全 `1`);
  - **`sortedByIndex` 判定必须比对方向**:排序规格与索引前缀逐字段比较「字段路径相同 **且** 方向相同」;方向不一致时 `sortedByIndex = false`(调用方照常做内存排序,结果正确);
  - wire 层 `createIndexes` 解析 `key` 的 `-1` 并落到 `directions`;`listIndexes` 按真实方向输出 `key`;
  - **索引条目扫描 API 增加 `descending: bool` 参数**,由规划器按索引方向的组合决定。混向复合索引(如 `{a:1, b:-1}`)在 MongoDB 里用于排序 `{a:1,b:-1}`,本引擎**先只保证全同向**(全 `1` 或全 `-1`)的降序扫描,混向时 `sortedByIndex` 保守返回 `false`,但索引仍可用于过滤。
- 验收:同一数据集建 `{a:1}` 与 `{a:-1}` 两个索引,`find_planned` 的 `sortedByIndex` 分别为 `true`/`true`(与方向匹配时),且**结果顺序必须与 `sort` 的期望完全一致**;不匹配时 `sortedByIndex=false` 且结果仍正确。

### 60. v0.5.4 M27 执行:降序索引

- **`IndexDef.directions: Vec<i8>`**:与 `fields` 一一对应(`1` 升序 / `-1` 降序);新增 `new_with_directions` / `is_descending()` / `is_uniform_direction()`;`to_json` 输出 `directions`,`from_json` **缺失时补全为全 `1`**(兼容 M26 及更早的索引定义)。
- **核心决策:降序索引的条目编码与升序完全相同,只有扫描方向相反。**
  若按降序编码(取反字节),范围区间语义、multikey 展开、唯一性检查全都要跟着改,风险远大于收益;而「逆序遍历升序索引」在 B+树上**等价于降序索引**。`scan_index_range` 新增 `descending: bool`,`true` 时对产出结果 `reverse()` —— 区间语义 `[lower, upper)` 不变。
- **`sortedByIndex` 判定必须比对方向**(这是 M27 要修的正确性缺口):
  1. 排序字段须是索引字段的**前缀**(顺序相同);
  2. 索引方向必须**单一**(混向索引无法用一次扫描满足两者 → 返回 `false`);
  3. 索引升序 ⇒ 排序须全升序;索引降序 ⇒ 排序须全降序。
  判定错误会**直接导致返回顺序错**且不报错,所以宁可保守。为此新增 `SortSpec::direction_signs()`,`sort_matches_prefix` 的签名由「字段列表」改为「整个 `IndexDef`」。
- **`find_planned` 跳过冗余排序**:`Plan::IndexScan { sorted_by_index: true }` 时不再做内存排序(索引已按序产出)。
- **wire 层**:`createIndexes` 解析 `key` 的 `-1` 落到 `directions`;`listIndexes` 按真实方向输出 `key`。新增 `Database::create_index_with_directions` 公开 API。
- 测试:
  - `tests/index_descending.rs` **9 例** —— 降序索引配降序排序(顺序断言)、**降序索引配升序排序必须 `sortedByIndex=false` 且结果仍正确**、升序索引配降序排序、降序 + skip/limit 分页、6 种条件与全表扫逐一对比、全降序复合索引、混向索引仍可过滤、方向持久化与旧定义默认值、更新后仍然正确;
  - Go 驱动 **+1 例**(`TestDescendingIndexOverWire`):经 wire 建 `{n:-1}` → `listIndexes` 报告方向 `-1` → 降序查询顺序为 `19,18,17`。
  - 合计 **466 例全绿**,Go **16 项**全通过,clippy 零警告。
- 踩坑记录:Go 侧 `listIndexes` 的 `key` 嵌套类型同样是 `bson.M` / `bson.D` 二义,直接 `m["key"].(bson.M)` 会得到 `nil`;改为递归查找 + `toInt64` 兼容 int32/int64。**跨语言测试不要假设嵌套 BSON 的具体表示类型**(M26 已在 `stage` 上踩过一次,这是第二次)。

### 61. v0.6.0 M28 设计:第一档语义补齐($exists/$type/$all/$size + 投影运算符)

- 背景:功能盘点(taolun #61 前)确认骨架完整,剩下的是**语义缺口**。第一档四项是 CMS 绕不过去的:字段存在性判断、数组完整包含、数组长度、投影里的元素级选取。
- **`$exists`**:`{f: {$exists: true|false}}`。语义:路径解析出**至少一个候选值**即「存在」;`false` 表示路径缺失。注意与 `$eq: null` 的区别——字段值为 `null` 时 `$exists: true` 成立而 `$eq: null` 也成立,但**字段缺失**时 `$exists: false` 成立、`$eq: null` 也成立(Mongo 的坑,需测试锁死)。
- **`$type`**:`{f: {$type: "string"|"objectId"|"date"|...}}` 或 `{f: {$type: ["string","null"]}}`。类型名对齐 MongoDB 别名(`double/string/object/array/binData/objectId/bool/date/null/int/long/decimal/minKey/maxKey/timestamp/regex`)。**`$type: "number"` 接受 int/long/double**;`$type` 对**缺失字段**不匹配(任何类型都不匹配)。
- **`$all`**:`{f: {$all: [v1, v2]}}`。语义:数组字段须**同时包含**全部元素(与 `$in` 的「任一」相反);若字段是标量则等价于「全部元素都等于该标量」;空数组 `$all: []` **不匹配任何文档**(MongoDB 语义)。
- **`$size`**:`{f: {$size: n}}`。语义:字段值须是**恰好**长度为 `n` 的数组;非数组或长度不等均不匹配。`n` 必须是**非负整数**,否则解析期报错。
- **投影运算符**(`src/query/projection.rs` 扩展,现有仅字段取舍):
  - `$slice`:`{"arr": {"$slice": n}}` 取前 n / 后 n;`{"arr": {"$slice": [skip, limit]}}`;
  - `$elemMatch`:`{"arr": {"$elemMatch": {...}}}` 只保留**第一个**匹配的元素(单元素数组);
  - 位置运算符 `$`:`{"arr.$": 1}` —— 需配合 filter 使用,**本期不做**(要回传 filter 匹配的下标,涉及投影与过滤的耦合,留 M29)。
- 纪律:全部改动只在 `filter.rs` / `projection.rs`,不动存储格式;`.nodb` 仍 v1;新增操作符必须与既有 `resolve_paths`(点号穿透数组)语义一致。
- 验收:`tests/filter_operators.rs` 逐操作符边界覆盖 + `tests/projection_operators.rs`;所有新操作符与「全表扫 vs 索引路径」结果一致(索引路径必须把新操作符算进候选集后二次过滤)。

### 62. v0.6.0 M28 执行:第一档语义补齐($exists/$type/$all/$size + 投影运算符)

- **`$exists`**:`Operator::Exists(bool)`。语义判据是「路径能否解析出候选值」——`values.is_empty() != want`。
  - **与 `$eq: null` 的关键区别**(测试锁死):字段**显式为 `null`** 时 `$exists: true` 成立、`$eq: null` 也成立;字段**缺失**时 `$exists: false` 成立,而 `$eq: null` **同样成立**(MongoDB 把缺失当 null 参与相等比较)。两者不等价,是 CMS 里最容易写错的地方。
  - 支持点号路径;参数必须是布尔值,否则解析期报错。
- **`$type`**:`Operator::Type(Vec<TypeName>)` + `TypeName` 枚举(14 个变体,对齐 MongoDB 别名,大小写不敏感)。
  - **`"number"` 别名展开为 Int/Long/Double 三项**,由 `parse_operator_object` 里显式展开(因为 `TypeName::parse` 是单射,别名需要在调用处展开)。
  - 接受单个字符串或字符串数组;非法类型名解析期报错。
  - **对缺失字段不匹配任何类型**(与 `$exists` 的行为差异,测试锁死)。
  - `object` 类型排除扩展 JSON 字面量(`$numberLong` 等),避免把数字字面量误判成对象。
- **`$all`**:`Operator::All(Vec<Value>)`,语义「数组须同时包含全部元素」(`$in` 的反面)。
  - **`$all: []` 不匹配任何文档**(MongoDB 语义,`required.is_empty()` 直接返回 false)。
  - 字段是标量时退化为「全部期望值都等于该标量」。
  - 复用 `value_matches` 做元素级比较,自动获得嵌套数组的递归语义。
- **`$size`**:`Operator::Size(usize)`,精确长度匹配;**非数组不匹配**(不是「长度等于」);负数/非整数解析期报错。
- **投影运算符**(`src/query/options.rs`,此前只有字段取舍):
  - `Projection` 新增 `operators: Vec<(path, ProjectionOp)>`;含 `$` 键的对象值**先于普通标志解析**,并自动加入包含字段列表(运算符天然属于包含模式);
  - `ProjectionOp::Slice { skip, limit }`:单值 `n`(负数为从尾部取 `|n|`)、双值 `[skip, limit]`;越界安全截断;**非数组原样返回**(不报错);
  - `ProjectionOp::ElemMatch(Box<Condition>)`:只保留**第一个**匹配的元素(单元素数组);无匹配返回空数组;内部条件**直接复用过滤器的 `Condition`**,语义与查询完全一致。
  - 为此 `Condition` / `Operator` / `ElemMatchSpec` / `FieldCondition` 全部加 `derive(Clone)`,`Condition::matches` 改 `pub`,并新增 `Condition::from_field_object`(把字段对象解析成完整条件,供投影复用)。
- 测试:
  - `tests/filter_operators.rs` **16 例** —— `$exists` 与 `$eq:null` 的区别(核心)、true 含 null/false/0/空串、false 只命中缺失、点号路径、参数校验;`$type` 六种基本类型、`number` 别名、**对缺失字段不匹配**、非法类型名;`$all` 全集包含、空数组不匹配、参数校验;`$size` 精确长度、非数组不匹配、非法参数;组合条件;**新操作符在索引路径与全表扫路径结果一致**。
  - `tests/projection_operators.rs` **11 例** —— `$slice` 四种形态(正/负/区间/越界)、保留 `_id`、非数组透传;`$elemMatch` 取首个/无匹配/多字段;非法参数;与普通字段混用。
  - Go 驱动 **+1 例**(`TestNewFilterOperatorsOverWire`):四个新操作符经 wire 协议逐一验证。
  - 合计 **493 例全绿**,Go **17 项**全通过,clippy 零警告。
- 明确未做:MongoDB 的位置运算符 `{"arr.$": 1}` —— 它需要回传 filter 匹配的**数组下标**,涉及过滤与投影的耦合,留 M29。

### 63. v0.6.1 M29 设计:数组更新 `$pull`/`$pop` + 投影位置运算符

- 背景:第一档(M28)补齐了过滤与投影的语义缺口。剩下三项是数组相关的真实缺口:`$pull`/`$pop`(数组更新)、位置运算符 `{"arr.$": 1}`(投影取 filter 命中的那个元素)。
- **`$pull`**:`{f: {$pull: <value>}}` 或 `{f: {$pull: {$in: [...]}}}` / `{$gt: 5}` / 文档条件。
  - 语义:从数组里移除**所有**满足条件的元素;字段不是数组时**报错**(与 `$push` 一致),字段缺失时**无操作**;
  - 值是「带 `$` 键的对象」→ 按**查询操作符**解释(复用 `filter::Condition`);否则按**等值**匹配(含数组元素级的 MongoDB 相等);
  - **返回是否实际改动**——与 M9 的「无变化不计入 modified」纪律一致。
- **`$pop`**:`{f: {$pop: 1}}` 移除最后一个,`{f: {$pop: -1}}` 移除第一个。
  - 只接受 `1`/`-1`(MongoDB 语义);空数组无操作;非数组报错;字段缺失无操作。
- **投影位置运算符 `{"arr.$": 1}`**:
  - 需要知道 **filter 在哪个元素上命中**,所以 `Projection::apply` 必须拿到 filter —— 接口从 `apply(doc)` 改为 `apply(doc, filter: Option<&Filter>)`;
  - 实现:`find_planned` / `find_with_options` 在应用投影时把 filter 传进去;投影内部对该路径的数组逐个元素用 filter 的**该字段条件**测试,取**第一个**命中的元素(与 `$elemMatch` 投影语义一致);
  - 路径写法 `arr.$`:解析时把末段 `$` 记为「位置占位」,不当作普通字段名;
  - **找不到匹配元素时返回空数组**(不报错),与 MongoDB 一致。
- 明确不做:数组过滤器 `$[]` / `$[identifier]`(需要额外语义,留后续)。
- 纪律:不动存储格式(v1);`$pull`/`$pop` 与既有「无变化不写回」的 `modified` 语义一致;投影接口变更需同步 C ABI 与 wire 层调用点。

### 64. v0.6.1 M29 执行:数组更新 `$pull`/`$pop` + 投影位置运算符

- **`$pull`**:`PullSpec { path, field, matcher: filter::Condition }`。
  - **两种形态**:值含 `$` 键 → 按查询操作符解释(复用 `Condition::from_field_object`);**普通对象 → 展开成字段条件做子集匹配**(`$pull: {r: {score: 8}}` 移除所有 `score == 8` 的对象,不要求对象只有这一字段);标量/数组 → 等值。
  - 字段缺失 → 无操作;非数组 → `TypeMismatch` 报错;`_id` 禁止。
  - **无元素被移除时不虚增 `modified`** —— 这条由既有的「内容无变化则不写回」机制自动保证,测试 `pull_no_change_not_counted` 锁死。
- **`$pop`**:`PopSpec { path, last }`。只接受 `1`(移除末尾)/ `-1`(移除开头);空数组与字段缺失均无操作;非法值解析期报错。
- **投影位置运算符 `{"arr.$": 1}`**:`ProjectionOp::Position { field }`。
  - **接口变更**:`Projection::apply(doc)` 保留,新增 `apply_with_filter(doc, Option<&Filter>)`;`find_with_options` / `find_planned` 的投影调用改传 `Some(filter)`。
  - **新增 `Filter::array_element_condition(field)`**:把 filter 里以 `数组字段.` 为前缀的条件**剥掉前缀**后返回,使其能对单个数组元素求值。支持顶层单字段条件与顶层 `And`(收集全部匹配项合成 `And`)。
  - 路径末段 `$` 不进入 `include_fields`,由 `operators` 单独处理;取值时用去掉 `$` 的真实路径。
  - **没有 filter、或 filter 无该数组的元素级条件时返回空数组**(MongoDB 语义),不是返回整个数组。
- 过程中踩的坑(值得记住):
  1. **`find_planned` 有第三处投影调用点**没被前面的批量替换命中,导致位置运算符在该路径下拿到 `filter=None` 而静默返回空数组。**用批量字符串替换改调用点时,必须事后 `grep` 全部调用点核对**,而不是相信替换的覆盖面。
  2. `Filter::field_condition` 最初插进了 `impl Condition` 而非 `impl Filter`(报 `no field root`),且语义也不对——需要的是「剥前缀」而非「字段名相等」。
- 测试:
  - `tests/array_update.rs` **17 例** —— `$pull` 标量/`$in`/范围/**对象子集**/字段缺失无操作/非数组报错/全删/`_id` 拒绝;`$pop` 首尾/空数组/缺失/非法值/单元素;`$pull`+`$push` 组合、无变化不计数、持久化。
  - `tests/projection_position.rs` **7 例** —— 命中元素、多个命中取首个、空数组、与其他字段共存、保留 `_id`、非数组透传、**索引规划路径下同样生效**。
  - Go 驱动 **+1 例**(`TestArrayUpdateOperatorsOverWire`)。
  - 合计 **517 例全绿**,Go **18 项**全通过,clippy 零警告。
- 明确不做:数组过滤器 `$[]` / `$[identifier]`。

### 65. v0.6.2 M30 设计:工程化收尾(Node 驱动 + 崩溃恢复 + 发布准备)

- 背景:第一档语义(M28/M29)已补齐,剩余是**敢让人用**所需的工程化验证。`cargo publish` 需维护者凭据,不在本里程碑内执行,只做准备与验证。
- **① Node.js 驱动兼容验证**:
  - 用官方 `mongodb` npm 包(与 Go 驱动同一套 wire 协议)做端到端验收,证明「协议层不是只对 Go 调过」;
  - 覆盖:连接握手、insertMany、find 游标分批、update/delete、`$pull` 新操作符、索引 `createIndexes`、`explain`、`countDocuments`;
  - 测试脚本放 `tests/node_compat/`,用 Rust 集成测试 `tests/node_wire.rs` 拉起 server 并执行 `node`(与 Go 的做法对齐:`go_compat` 已有先例)。**若本机无 `mongodb` npm 包装成功则跳过并显式提示**,不静默通过。
- **② 崩溃恢复 / 一致性测试**(`tests/crash_recovery.rs`):
  - **未提交写事务不得可见**:模拟「写入过程中不 commit」,重开后数据必须回到事务前状态(redb 的原子性由引擎保证,这里验证我们的封装没破坏它);
  - **写一半进程被杀**:用子进程执行「插入 N 条 + 立刻 `std::process::abort()`」,父进程重开库校验**要么全部 N 条、要么一条没有**(不允许部分写入);
  - **损坏文件拒绝打开**:截断 `.nodb` 文件、翻转中间字节、写入随机垃圾 → 必须返回明确错误而不是 panic 或返回空数据;
  - **索引与文档一致性**:崩溃后用 `explain` 验证索引仍可用,且索引查询结果与全表扫一致。
- **③ 发布准备**:
  - `Cargo.toml` 元数据复核(`description`/`keywords`/`categories`/`repository`/`license`/`rust-version`);
  - `cargo publish --dry-run` 通过;
  - README 补 Node 驱动片段;
  - CHANGELOG 汇总 v0.1.0 → v0.6.1。
- 纪律:不改存储格式(v1);测试新增文件按功能拆分;崩溃测试必须用**真实进程终止**而非 mock,否则验证不到 fsync 边界。

## Agents

### 规范

1. 一个问题重复 3 次无法解决完成 → 强制停止,向用户详细汇报遇到的问题,等待用户解答。
2. 整个对话流程强制使用中文,包括 AI 打印在终端中的内容(think 输出)。
3. 项目必须有详细的中文注释。
4. 版本发布格式:`YYYY.MM.DD.xxxx`,`xxxx` 为作为 git tag 前的 commitID,便于溯源。
5. 写测试时,测试文件按功能模块拆分成多个文件,禁止在一个文件里写全部内容。
6. 开发采用 OOP 面向对象方式(Rust 惯用 `struct + impl + trait`),模块单一职责,高内聚低耦合。

### 双形态路线(v0.2 起的总纲)

> 定位:**单文件嵌入式 + MongoDB 语义 100% 兼容。**
>
> **关键澄清(v0.4 讨论确立)**:「像 SQLite」只约束**部署形态**,不约束**数据模型与工程取舍**。
>
> | 维度 | 学谁 | 说明 |
> |---|---|---|
> | 部署形态(单文件 / 进程内 / 零配置) | **SQLite** | 这是「像 SQLite」的唯一含义 |
> | 数据模型 / 查询语义 / 索引模型 / 游标 / 协议 | **MongoDB** | 目标 100% 对齐,**不得**因为「嵌入式」而降级 |
> | 引擎内部实现手法(如持有活游标的 `stmt` 式扫描) | 可借 SQLite 工程手法 | 只是实现手段,不得改变对外语义 |
>
> **nosqlite 是 SQLite 在文档领域的对立面竞争者,不是「另一款 SQLite」。**
> 因此索引必须按 MongoDB 的模型做(multikey、复合索引前缀原则、BSON 类型序),
> 绝不能采用关系型引擎的取舍。

- **形态 A —— 嵌入式(C ABI)**:`.nodb` 由宿主进程独占,像 SQLite 一样被任何语言**进程内**调用。
  - 适用:本地 GUI/App(Electron/Tauri/Go/Wails)、CLI 工具、桌面软件。
  - 硬要求:不能让端侧用户额外安装/常驻服务进程。
- **形态 B —— 服务端(wire 协议)**:`nosqlite-server` 监听 `mongodb://`,**第三方 MongoDB 官方/社区 driver 直连**。
  - 适用:多进程/多语言共享、Web 后端(如 CMS)。
  - 目标:客户端无需任何 nosqlite 专属 SDK。
- 共同基础:查询语义、Extended JSON、错误码 —— **两者共享,不得分叉**。

#### 查询语义总目标:**尽可能完整地支持最新 MongoDB 查询语言**

- 当前实现的是「满足驱动/ORM 调用面的**最小集**」,**不是终点**。
- **长期目标:尽可能完整地支持最新版 MongoDB 的查询/更新/聚合语义**,包括但不限于:
  - 查询:已实现 `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex/$elemMatch/$not`(字段级+顶层)、`$and/$or/$nor`;
    待补 `$exists/$type/$all/$size/$mod/$expr/$text/$where`、`$regex` 的 `$options`、地理查询等。
  - 更新:已实现 `$set/$unset/$inc/$mul/$min/$max/$rename/$push/$addToSet/$currentDate`;
    待补 `$pull/$pop/$setOnInsert/$bit/$push` 全部修饰符、数组过滤器 `$[]/$[id]` 等。
  - 聚合管道:`$match/$group/$project/$sort/$limit/$skip/$unwind/$lookup/$facet/$addFields/$replaceRoot/...`。
- 纪律:每一项补齐都必须**以 MongoDB 官方文档为唯一语义基准**,并用真实 driver 行为回归验证;
  已实现的语义不得回退(现有测试即护栏)。
- 判定优先级:**被真实 driver/ORM 隐式调用到的语义优先**(如 Mongoose 生成的 `$exists`),而非文档里看起来常用的。

### 项目进度

#### 计划中
- **阶段 1.5 已完成**(v0.2.2)。尚未做(等有真实下游再投入):macOS 签名/notarization、npm & go module 分发元数据、Windows MSVC 运行时说明。
- **阶段 2 已完成**(v0.3.0),真实游标已完成(v0.3.1)。后续按需:认证、完整 `$group` 累加器、Node 驱动验证、`mongosh` 兼容、游标随连接断开自动清理。
- **M30(下一里程碑,发布收尾)**:`cargo publish`、Node.js 驱动验证、崩溃恢复/一致性测试、索引统计(按需)。
- **待补**:`drop_collection` 应同时清理该集合的索引定义与条目(当前只在重建索引时对齐)。
- **M24**:查询规划器 —— 等值/范围/排序/复合前缀走索引 + `explain`。
- **M25**:唯一/稀疏/TTL 索引、`listIndexes`/`dropIndexes`、游标超时清理。
- **阶段 3**:语义向 MongoDB 完整靠拢(见「双形态路线 → 查询语义总目标」)。
- **语义长期目标**:向最新 MongoDB 完整靠拢(见「双形态路线 → 查询语义总目标」)。
- crates.io 发布:干跑已通过,**按用户决定暂缓**(GitHub Release v0.1.0 已发);待需要 crates.io 分发时 `cargo login` 后 `cargo publish` 即可。
- CMS 项目启动后进入维护模式:真实场景验收(CMS 作为第一个真实用户)优先于新功能。
- 后续(按需触发):索引下推/排序下推(单集合超大数据量时)、Server 模式(触发条件:出现「多个独立服务共享同一库」的真实需求;实现顺序:HTTP/axum 包一层 → C ABI/FFI → MongoDB wire 协议)、聚合管道。
- **跨语言访问路线(v0.2 起重定)**:见下方「双形态路线」章节。旧结论「先 HTTP 再 C ABI」已作废 —— 端侧(App)的核心价值恰恰是**嵌入式**,不能要求用户装服务。

#### 代办
- (无 —— M11 已完成，CLI 同步并入 M10 或发布前置项)

#### M11 验收指标(执行结果见 taolun #25)

| 改造项 | 现状(改造前) | 目标 | 实测 | 结果 |
|---|---|---|---|---|
| `insert_many` 单事务 | 43.7–238.8 条/秒 | ≥ 40,000 | 44,153(macOS)/298,683(Linux) | ✅ |
| 主键寻址 | 500k 档 2.6 s | ≤ 1 次 fsync 下界(原≤1ms 指标定义有误) | 20.7 ms(macOS)/0.82 ms(Linux @100k) | ✅ |
| 流式化/`find_page` | RSS 增量 = 物化 10–12 倍 | ≤ 2 倍 | 500k 档 **30.2 MiB**(27x 改善) | ✅ |
| `find` 全量延迟 | 7.3–8.8 µs/条 | 不劣化 | 8.25 µs/条(macOS)/1.19(Linux) | ✅ |

#### 已完成
- M29(v0.6.1):`$pull`/`$pop` + 投影位置运算符 `arr.$`;517 例全绿。
- M28(v0.6.0):`$exists`/`$type`/`$all`/`$size` + 投影 `$slice`/`$elemMatch`;493 例全绿。
- M27(v0.5.4):降序索引;修正 sortedByIndex 未比对方向的顺序错缺口;466 例全绿。
- M26(v0.5.3):`$or` 并集计划、多索引交集计划、wire createIndexes/listIndexes/dropIndexes;457 例全绿。
- M25(v0.5.2):复合索引 + wire explain;449 例全绿。
- M24(v0.5.1):查询规划器 —— 等值/范围走索引 + explain;440 例全绿。
- M23b(v0.5.0):索引框架 —— 编码 + 独立索引表 + multikey/稀疏/唯一 + 写路径维护;427 例全绿。
- M23a(v0.4.1):类型序对齐 MongoDB 官方 BSON 序 + MinKey/MaxKey/Timestamp;392 例全绿。
- M22(v0.4.0):ScanSession 真流式(活游标,自持读事务);382 例全绿。
- M21(v0.3.1):真实游标 —— `getMore`/`killCursors`/`batchSize`,`limit` 语义修正;372 例全绿。
- M20(v0.3.0,阶段 2):nosqlite-server(MongoDB wire 协议),Go 官方驱动验收通过;367 例全绿。
- M19(v0.2.2,阶段 1.5):C ABI 收尾 —— `open_ex` + 三平台库产物 + 分发文档;354 例全绿。
- M18(v0.2.1,阶段 1):C ABI(`nosqlite-ffi`)—— 句柄/错误码/游标 + arena 结果 + 结构化错误;真实 C 与 Python 双验证;354 例全绿。
- M17(v0.2.0,阶段 0):跨语言契约冻结 —— 字段序 / 错误码 / Extended JSON;351 例全绿。
- M16(v0.1.5):CLI `.export`/`.import`/`.stats`;335 例全绿。
- M15(v0.1.4):更新操作符扩展(8 个)+ upsert;325 例全绿。
- M14(v0.1.3):Date 类型 —— `{"$date": ...}` 按时间戳比较排序(时区归一),格式仍 v1;275 例全绿。
- M13(v0.1.2):查询语义与 MongoDB 对齐 —— 同层 AND、数组等值/`$in` 展开、点号路径穿透数组、缺失匹配 null、字段级 `$not`;258 例全绿。
- M12(v0.1.1):A 组四项 —— 批量 `update`/`delete` 单事务(310x)、字段投影、`find_page_total` 一次扫描、array/object 递归比较;235 例全绿。
- M10:`$elemMatch`(操作符/文档双形态)+ `$unset`(对齐 MongoDB,数组置 null),+15 例共 194 例全绿;CLI 同步 M11 能力(`find --sort` → `find_page`、`insert` 数组 → `insert_many`)。
- M11:内存优先改造 —— `find/count/update/delete` 流式化;`find_page` 两遍法;`get/update/delete_by_id` 主键寻址 + `{"_id":等值}` 自动快速路径;`insert_many` 单事务批量;`Storage` trait 增 `for_each/get/insert_many`;179 例全绿;跨平台基准确认 macOS `F_FULLFSYNC` 放大 66x;GitHub Actions CI 建立。
- M9:缺陷修复(#1 数组下标路径数据破坏、#2 空更新异常)+ 压测基线(写入吞吐 43.7–238.8 条/秒且随文件增大恶化;全量 find 内存放大 6–12 倍,50 万档 819 MiB)。
- M8:REPL 对齐 sqlite3 点命令(管理一律点号、CRUD 无点、`.collect` 列集合)。
- M7:CLI 默认 TTY(对齐 sqlite3,无参/带路径直接进 REPL + 自动创建,移除单命令模式)。
- M6:CLI 交互式 shell(`nosqlite shell <file.nodb>` REPL,提示符 `nosqlite> `,shell 内命令不带文件路径)。
- M5:CLI 全命令(count/collections/drop)+ 单文件复制粘贴自动化验证 + 测试补全(112 例全绿)。
- M4:update/delete,CRUD 闭环(`$set` 更新操作符、`Storage` trait 扩展 update/delete、CLI update/delete)。
- M3:find + filter 子集($eq/$gt/$in/$regex)+ sort/limit/skip。
- M2:文档模型 + insert + 主键索引(自动/显式 `_id`,重复主键拒绝,集合隔离)。
- M1:项目骨架 + AGENTS.md + `.nodb` 创建/打开(魔数头 + 版本校验,拒绝非法后缀)。

### 开发流程

1. 每次先保存讨论记录(追加到 `## taolun`),再开始改动文件内容。
2. Windows 已内置 coreutils,可用 `grep/ls/seq/sed/find/sleep` 等命令,无需 PowerShell/cmdlet。注意 `cat/ls/mv/rm/echo/sleep` 等与 PowerShell alias 冲突的命令须带 `.exe` 后缀。
3. 开发完成后,更新「项目进度」与「changelog」;changelog 条目需外链到对应 taolun 记录和项目进度栏目,便于溯源。

## 认知修正

> AI 开发过程中的踩坑记录、用户纠正与明确结论,避免重复死循环。

- **写入基准必须标注平台(macOS `F_FULLFSYNC` 放大)**:Rust std 在 macOS/iOS 把 `sync_all()`/`sync_data()` 映射到 `F_FULLFSYNC`(强制磁盘写缓存落物理介质),代价是普通 `fsync` 的 **66 倍**(实测 D 组 4.80 ms vs G 组 POSIX `fsync` 0.072 ms);Linux 上映射到 `fdatasync`,无放大(D/G = 1.0x)。因此逐条写入 185 条/秒是 macOS 最坏值,Linux 实测 1,382 条/秒。**引用任何写入数字必须标注平台**;「fsync 是下界」的表述只在 macOS 成立。
- **归因必须在同进程内做对照组**:曾用不同进程/不同时间的运行结果对比,得出「`insert_many` 差 3x、序列化占主导」的错误结论;同进程同批次对照(bench_write F vs B)后证明两者持平(macOS 1.02x,Linux 达理论上限 95%)。机器噪声远大于微秒级差异,跨运行数字不可用于归因。
- Windows 下 PowerShell alias 会覆盖 coreutils 的 `cat/ls/mv/rm/echo` 等命令,调用时必须加 `.exe` 后缀(如 `ls.exe`、`cat.exe`)。
- 存储引擎决策路径是"先现成后自研":不要一开始就自研文件格式,先用 redb 跑通,再通过 `Storage` trait 替换。
- OOP 要求不是传统类继承,在 Rust 里用 `struct + impl + trait` 表达。
- `.nodb` 后缀是硬性要求:任何平台、任何场景都不接受其他后缀或无后缀。
- Windows 下 redb 持有文件独占锁:同一进程内"创建后再打开"必须先 `drop` 释放第一个句柄,否则 `Database::open` 会因锁冲突报"文件不存在或不是有效数据库"的误导性错误。
- PowerShell 双引号 here-string(`@"..."@`)会插值 `$` 变量(如 `$set`/`$gte` 被清空导致 filter/update 解析异常),多行命令注入 stdin 冒烟时必须用单引号 here-string(`@'...'@`)。
- sqlite3 CLI 语义:默认直接进入 TTY,且不依赖数据库文件是否存在——不管有没有库都能进入,在 TTY 内用 `.open` 等命令创建/加载数据库;对齐目标是「无参数进 REPL、带路径打开(不存在即建)、移除单命令子命令」。
- **写性能的归因结论(不要重复踩坑)**:nosqlite 逐条插入的耗时 ≈ 一次 fsync 的物理成本(A/D = 1.08x @1,000 条、0.76x @10,000 条),**不是引擎实现缺陷**。且 fsync 成本随数据文件增大而恶化(3.87 ms @1,000 条 → 30.20 ms @10,000 条),所以逐条写事务的吞吐会随数据库增长而下降(238.8 → 43.7 条/秒)。修法是**批量单事务**(收益 314x～1562x),不是换引擎。
- **压测方法**:单次测量会被机器负载污染 —— 同一「1,000 条逐条插入」曾测出 4.19 s 与 23.1 s 两种结果。压测必须**预热 + 重复多次取中位数并报极差**,否则数据不可用于决策。
- **MongoDB 驱动把批量数据放在 OP_MSG section 1,不在命令文档里**:`insert` 的文档在 identifier 为 `documents` 的 section 1,`update`/`delete` 分别是 `updates`/`deletes`。只解析 section 0 会表现为「操作成功但影响 0 条」,且驱动不报错,极难排查。
- **wire 协议里 `cursor.id` 必须是 BSON int64**:写成普通数字会被编码成 int32,驱动直接拒绝整个响应(`id should be an int64 but it is a BSON 32-bit integer`)。用 `{"$numberLong": "0"}` 强制 int64。
- **`maxWireVersion` 给低了驱动直接拒连**:当前给 17。改小前务必用真实驱动验证。
- **ctypes 不声明 `argtypes` 会导致 64 位指针截断**:`ctypes.CDLL` 默认把未声明的参数当 `int` 传递,`nosqlite_db*` 句柄被截断成 32 位后必然段错误。凡是接受句柄的函数,必须显式 `fn.argtypes = [ctypes.c_void_p, ...]`。排查时表现为「单独调用正常,组合调用崩溃」。
- **读路径的真问题是内存不是延迟**:`find` 全量延迟严格线性(7.3–8.8 µs/条,可预期),但 `scan` 返回 `Vec<(String, Vec<u8>)>` 导致 RSS 增量稳定为文档总字节的 **10–12 倍**(50 万条达 819 MiB)。优化优先级应为**内存 > 写入吞吐 > 读延迟**。
