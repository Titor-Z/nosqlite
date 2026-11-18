# Changelog

版本号采用语义版本。完整设计与决策记录见 [AGENTS.md](./AGENTS.md) 的 `taolun` 章节。

## [0.7.1] - 2026-11-16

角色授权:认证之后补上授权,按 MongoDB 的 6 个内置角色做集合级权限。

### 角色
- `read` / `readWrite` / `dbAdmin` / `userAdmin` / `dbOwner` / `root`,
  未知角色名解析期报错;
- 检查落在 **wire 边界**(嵌入式/CLI 仍不认证 —— 能打开文件就是全权限);
- `createUser` 的 `roles` 参数真正生效(此前被忽略,一律建成 `readWrite`);
  `updateUser` 支持只改密码或只改角色;`roles: ["read"]` 字符串简写也接受;
- `usersInfo` 加细规则:查自己永远可以,查别人要 `userAdmin` / `root`;
- `connectionStatus` 返回真实角色。

### 安全修复
- 三个凭据读旁路被堵死:`distinct admin.system.users credentials`、
  `aggregate` 直接打在 `admin.system.users`、`$lookup` / `$unionWith` 指向它
  (`$facet` 子管道递归检查);`find` 保持放行但响应前剥掉 `credentials`;
- 修掉「账号管理命令绕过角色闸」—— `createUser` / `updateUser` / `dropUser`
  在 dispatch 的认证命令分支提前返回,跳过了统一权限检查,导致 `readWrite` 账号也能建号;
- 修掉「未知角色被静默写进库」—— 校验只做在 wire 解析层,CLI `.user add <name> superuser`
  绕过了它;现改在 `UserStore` 层校验,所有入口一起覆盖。

### npm 包
- 补上三个此前有 C ABI 却没暴露的接口:`aggregate`(聚合管道)、`dropCollection`、
  `compact`(文件压缩)—— 绑定 + 封装 + 类型声明 + README + 测试一起补齐;
- 嵌入模式不暴露账号/角色(能打开文件即全权限),README 写明该场景用 `nodblite-server`;
- npm 测试 15 项全过,`tsc -p tsconfig.json` 通过。

### 测试
- 630 例 Rust 全绿;真实驱动:Go 9 项、Node 8 项带角色验收(只读账号写/建索引被拒、
  readWrite 建号被拒、userAdmin 建号与看别人通过、`$lookup`/`distinct`/`aggregate`
  读凭据被拒)。

参见 AGENTS.md taolun #85–#86。

## [0.7.0] - 2026-11-16

聚合 / 查询 / 更新语义补齐、文件压缩、账号认证(SCRAM-SHA-256)。

### 聚合(`src/aggregate.rs` 重写为逐阶段执行)
- 新增 `$unwind` / `$lookup`(用 `$in` 一次查询,无 N+1)/ `$facet` / `$bucket` / `$replaceRoot`(`$replaceWith` 别名)/ `$count` / `$unset`;
- 累加器补到 9 个:`$sum` / `$avg` / `$min` / `$max` / `$first` / `$last` / `$push` / `$addToSet` / `$count`;
- 首 `$match` 下推存储层;未实现的 stage 明确报 `40324`,不静默返回空。

### 查询
- `$expr`(`src/query/expr.rs`,25 个表达式操作符,`$project` / `$addFields` / `$group._id` / 累加器共用同一求值器);
- `$mod`;`$regex` 支持 `$options`(`i/m/s/x`)。

### 更新
- `$bit`(`and` / `or` / `xor`);
- 数组位置运算符 `$[]` / `$[id]`(展开成具体下标路径,复用既有赋值逻辑)。

### 运维
- `.compact` 文件压缩(CLI + C ABI `nodblite_compact`):删除不缩文件是 redb 语义(空闲页复用),压缩是显式能力,不做自动后台触发;加密不做(指引 OS 级全盘加密);
- `.stats` 维持原口径 —— 实测否掉 redb `fragmented_bytes` 当「空洞率」指标(方向反了),宁可只给压缩前后真实大小。

### 账号认证(SCRAM-SHA-256)
- 官方 Go / Node 驱动带账号密码直连验收通过;
- 账号存真实集合 `admin.system.users`(`admin` 为保留命名空间,建库即写 `admin.system.version`),写路径拒绝 `system.*`、读时剥 `credentials`;
- 库内无账号 = 不认证(等同 `noauth`);第一个账号自动 `root`;
- 只做**认证**不做授权:通过认证即全权限,`roles` 字段已就位但暂不检查。

### 测试
- 626 例 Rust 全绿(新增 `tests/auth.rs` / `tests/auth_wire.rs` / `tests/compact.rs` / `tests/aggregate_stages.rs` 等);
- 真实驱动:Go 18+4 项、Node 16+4 项、Mongoose 9 项。

参见 AGENTS.md taolun #81–#84。

## [0.6.11] - 2026-11-16

nodblite-studio:对标 MongoDB Compass Data Explorer 的单二进制 Web 管理台。

### 变更
- **暗色模式**:三态(跟随系统 / 亮 / 暗),`data-theme` 令牌覆盖,首屏脚本避免闪烁;新增 `on-accent / shadow / th-bg / th-fg` 语义令牌处理暗色反色。
- **查询栏精简**:Reset / Explain / Options / History 收进 `⋮` 二级菜单,Find 保留主按钮。
- **表格视窗高度**:结果区高度 = 视窗剩余高度,表头 sticky 固定,横向滚动条常驻可见。
- **编辑器**:JSON 语法高亮(零依赖);`Fields` 视图按类型(含 date/array/object)编辑字段并校验,避免写错值类型。
- **select 样式化**:`appearance: base-select` + `::picker(select)`(Chrome 135+),弹出列表跟主题走;不支持的浏览器自动退回原生外观。
- **视觉系统改为构成主义**:纸底 `#f2ede3` + 墨黑 `#12100e` + 朱红 `#d6281e`;粗墨线、直角、斜切标签块、墨底表头、硬阴影浮层。移除扫描线、光晕、渐变、圆角与切角。正文改无衬线,数据区保留等宽。

### 文档站与 Playground
- **视觉改为苏联构成主义海报**:首页 `layout: false` + 自建 `HomePoster.vue` —— 斜向红条、巨型全大写标题 `NODBLITE`、红底错位标签、硬框内嵌物理构图(图注「图 01」)、数字条、01–06 规格网格、红黑对置「用它 / 别用它」、墨底 `ONE FILE. NO SERVER.` CTA;首页自带明暗切换(与文档站共用 localStorage)。
- **文档页换皮**:纸/墨/朱红令牌、圆角归零、线宽三档(1 / 1.5 / 2px + 4px 强调)、导航大写加字距、侧栏活跃项列表外短朱红条、h2 以 2px 墨线分节、表头深底浅字、内联代码朱红字;代码高亮换 `min-light / min-dark`;logo、favicon 重画。
- **官网部署 Playground(`/playground/`)**:studio 静态产物随文档站一起发布,打开网页拖入 `.nodb` 即可查看集合/文档/索引/查询计划。wasm 引擎在本页内运行,数据不出本机;编辑器为临时查看,刷新即丢。`site/.vitepress` 与 studio 共用一份构建(`base: './'`),不再需要两套产物。
- **发布物**:Release 新增 `nodblite-studio-<platform>` 单二进制(Linux x86_64 / macOS aarch64 / Windows x86_64 + SHA256),供长期读写磁盘文件使用。
- **文档纠正**:JS/TS 快速开始改为以官方 npm 包 `nodblite` 为主线(原为裸 koffi FFI 教程);wasm 独立成「快速开始 → wasm」一页,列明 Node 用 `node:wasi`、浏览器需自备 WASI、以及 wasm 侧缺少的 `findPage / explain / upsert / 复合索引`。
- **窄屏修复**:playground 顶栏文字重叠、工作区被固定 `270px` 左树挤成竖条。

### 新增
- **`nodblite-studio`(重构)**:信息架构对齐官方 Data Explorer —— 左侧 Databases 树(集合+文档数)、单集合工作区、二级标签 `Documents / Indexes / Explain / Schema / Aggregation`。
- **Documents**:查询栏(Filter/Project/Sort/Options/Find/Reset/Explain/History)、Table⇄JSON 双视图(字段并集列、行内嵌套展开、列头排序)、`Documents X–Y of N` 翻页、Insert Document / Insert Documents / 复制文档 / 行内编辑删除。
- **集合级 IO**:Import File(JSONL/JSON 数组,先校验后单事务写入)、Export(JSON/JSONL/CSV,分批不整集合入内存)。
- **Indexes**:官方表格(Name/Fields/Type/Size)+ IndexBuilder(动态字段-方向行、unique/sparse)。
- **Explain**:计划阶段链 + Raw JSON;指标区诚实标注 `CLIENT MEASURED`(引擎不提供扫描计数)。
- **Schema**:样本量 100/200/500/1000 采样,字段出现率与类型分布。
- **Aggregation**:stage 卡片编辑器(增删/上下移)、前缀管道逐 stage 预览、结果复用 Table/JSON 视图。
- **Rust**:`src/aggregate.rs` 提取共享聚合执行器(wire 与 C ABI 共用);C ABI 新增 `nodblite_aggregate`、`nodblite_drop_collection`;`serve` 拆出 `serve_listener` 便于测试。
- **降级标注**:Collation/Max Time/索引 Size·Usage/hidden·partial·TTL 引擎不支持,UI 显式标注 `—` 或 `NOT SUPPORTED`,不静默假装。

### 构建
- `studio-ui/`:Svelte 5 + Tailwind CSS 4 + Vite 8(bun 安装/构建)。
- `scripts/build-studio.sh`:wasm → 前端 → 单二进制的固定顺序构建。
- 普通 `cargo build/test` 不受影响(`studio` feature + `required-features` 隔离)。

### 测试
- `tests/studio_http.rs` 5 例;`cargo test --features studio` 531 例全绿(默认 526)。
- `studio-ui/e2e/smoke.mjs`:真实 Chrome 28 项全过(对标 Data Explorer 的完整工作流)。

## [0.6.10] - 2026-10-21

文档更正。

### 文档
- npm README 的支持平台补上 macOS x64(此前只写 arm64,与实际预编译产物不符)。
- npm README 的 CommonJS 说明改为按实际行为表述:纯 ESM 包能否直接 `require()` 取决于 Node 版本,遇到 `ERR_REQUIRE_ESM` 时改用动态 `import()`。

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
