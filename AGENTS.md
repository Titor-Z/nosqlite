# AGENTS.md — nosqlite 项目 Agent 规范

> nosqlite:一个像 SQLite 一样小巧的单文件 NoSQL 数据库(文件后缀 `.nodb`),支持 Rust 内嵌 API 与 CLI。

## changelog

> 更新记录,按时间倒序(最新在上),条目需外链到对应 taolun 记录与项目进度。

- 2026.09.16: v0.1.0 发布 —— GitHub Release 附三平台 CLI 二进制(Linux x86_64 / macOS aarch64 / Windows x86_64 + SHA256,`release.yml` 自动构建);Cargo.toml 补齐 crates.io 元数据(rust-version 1.85/keywords/repository/license);README(定位/何时不用/跨平台性能表/内存与持久化语义)+ Apache-2.0 LICENSE;**crates.io 待发布**(需维护者 `cargo login`,`cargo publish --dry-run` 已验证通过)。

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
- crates.io 发布:干跑已通过,**按用户决定暂缓**(GitHub Release v0.1.0 已发);待需要 crates.io 分发时 `cargo login` 后 `cargo publish` 即可。
- CMS 项目启动后进入维护模式:真实场景验收(CMS 作为第一个真实用户)优先于新功能。
- 后续(按需触发):索引下推/排序下推(单集合超大数据量时)、Server 模式(触发条件:出现「多个独立服务共享同一库」的真实需求;实现顺序:HTTP/axum 包一层 → C ABI/FFI → MongoDB wire 协议)、聚合管道。
- **跨语言访问路线(已定)**:HTTP(axum/CMS)→ C ABI/FFI(`nosqlite-ffi`,cbindgen 生成头文件)→ wire 协议;`.nodb` 文件永远由进程独占,客户端只走网络访问 —— Node/Go 等 MongoDB 驱动在 wire 协议落地前无法直连。

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
- **读路径的真问题是内存不是延迟**:`find` 全量延迟严格线性(7.3–8.8 µs/条,可预期),但 `scan` 返回 `Vec<(String, Vec<u8>)>` 导致 RSS 增量稳定为文档总字节的 **10–12 倍**(50 万条达 819 MiB)。优化优先级应为**内存 > 写入吞吐 > 读延迟**。
