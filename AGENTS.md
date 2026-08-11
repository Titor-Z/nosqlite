# AGENTS.md — nosqlite 项目 Agent 规范

> nosqlite:一个像 SQLite 一样小巧的单文件 NoSQL 数据库(文件后缀 `.nodb`),支持 Rust 内嵌 API 与 CLI。

## changelog

> 更新记录,按时间倒序(最新在上),条目需外链到对应 taolun 记录与项目进度。

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

## Agents

### 规范

1. 一个问题重复 3 次无法解决完成 → 强制停止,向用户详细汇报遇到的问题,等待用户解答。
2. 整个对话流程强制使用中文,包括 AI 打印在终端中的内容(think 输出)。
3. 项目必须有详细的中文注释。
4. 版本发布格式:`YYYY.MM.DD.xxxx`,`xxxx` 为作为 git tag 前的 commitID,便于溯源。
5. 写测试时,测试文件按功能模块拆分成多个文件,禁止在一个文件里写全部内容。
6. 开发采用 OOP 面向对象方式(Rust 惯用 `struct + impl + trait`),模块单一职责,高内聚低耦合。

### 项目进度

#### 计划中
- 后续:Server 模式(协议让 TS/Go 可访问)、聚合管道、完整 MongoDB 兼容。

#### 代办
- 无(当前 M5 已完成,待 Server 模式)。

#### 已完成
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

- Windows 下 PowerShell alias 会覆盖 coreutils 的 `cat/ls/mv/rm/echo` 等命令,调用时必须加 `.exe` 后缀(如 `ls.exe`、`cat.exe`)。
- 存储引擎决策路径是"先现成后自研":不要一开始就自研文件格式,先用 redb 跑通,再通过 `Storage` trait 替换。
- OOP 要求不是传统类继承,在 Rust 里用 `struct + impl + trait` 表达。
- `.nodb` 后缀是硬性要求:任何平台、任何场景都不接受其他后缀或无后缀。
- Windows 下 redb 持有文件独占锁:同一进程内"创建后再打开"必须先 `drop` 释放第一个句柄,否则 `Database::open` 会因锁冲突报"文件不存在或不是有效数据库"的误导性错误。
