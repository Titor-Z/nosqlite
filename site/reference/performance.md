---
title: 性能报告
description: nodblite 的完整实测数据 —— 读、写、索引、内存、并发、平台差异,每个数字都标注口径
outline: [2, 3]
---

# 性能报告

::: info 一句话结论
**读取是数量级地快,写入取决于你怎么写。**
单条插入每次都要落盘,批量插入快两个数量级;索引对高选择率查询有 300–4000 倍收益,但对低选择率(命中 >10%)收益有限。
:::

::: warning 引用数据前请读这句
写入数字**强依赖平台的落盘(fsync)语义**。macOS 的 `F_FULLFSYNC` 比 Linux 的 `fdatasync` 慢 **66 倍**,同一份代码在两地可以差一个数量级。**引用任何写入数字都必须标注平台。**
:::

## 关键数字

<div class="nb-stats">
  <div class="nb-stat">
    <div class="value">7.98<span class="unit">ms</span></div>
    <div class="label">全量扫描 1,000 条</div>
  </div>
  <div class="nb-stat">
    <div class="value">44,000<span class="unit">条/秒</span></div>
    <div class="label">批量写入(macOS)</div>
  </div>
  <div class="nb-stat">
    <div class="value">500<span class="unit">×</span></div>
    <div class="label">唯一索引等值查询</div>
  </div>
  <div class="nb-stat">
    <div class="value">30.2<span class="unit">MiB</span></div>
    <div class="label">50 万条分页内存</div>
  </div>
</div>

## 测试方法

::: details 基准环境与统计口径
- **平台**:macOS Apple Silicon(本机);Linux 数据来自 GitHub Actions 共享 runner,仅作指示性参考
- **文档结构**:每条约 160 字节(含 120 字节 payload 字符串)
- **统计**:预热后重复 3 轮,取**中位数**并记录极差
- **数据载入**:读基准用批量单事务直接写入,避免逐条插入的落盘成本污染测量
- **命令**:

```bash
cargo run --release --example bench        -- 10000 100000
cargo run --release --example bench_write  -- 10000 3
cargo run --release --example bench_index  -- 100000
```
:::

::: tip 为什么强调方法
单次测量会被机器负载污染 —— 同一个「1000 条逐条插入」我们曾测出 **4.19 s** 与 **23.1 s** 两个结果(差 5 倍)。压测必须**预热 + 重复 + 取中位数 + 报极差**,否则数据不能用于任何决策。

更极端的是 fsync 本身:同一文件、同样脏页量连续 30 次,`min 21 ms / max 1140 ms`(**54 倍方差**)。所以涉及落盘的对比一律用 **min 或 p10** 作判据,不用中位数。
:::

## 读取性能

### 全量扫描:严格线性

<PerfTable
  caption="find 全量 —— macOS,每条约 160 字节"
  :columns="['规模', 'find 全量', '每条约', '内存增量']"
  :rows="[
    ['1,000',   { text: '7.98 ms',   num: true }, { text: '8.0 µs', num: true }, { text: '2.1 MiB',   num: true }],
    ['10,000',  { text: '73.28 ms',  num: true }, { text: '7.3 µs', num: true }, { text: '18.2 MiB',  num: true }],
    ['100,000', { text: '775.38 ms', num: true }, { text: '7.8 µs', num: true }, { text: '213.6 MiB', num: true }],
    ['500,000', { text: '4.385 s',   num: true }, { text: '8.8 µs', num: true }, { text: '526–819 MiB', num: true }],
  ]"
/>

::: danger 旧接口 scan() 的内存陷阱
上表「内存增量」是**旧接口 `scan()`** 的数据 —— 它把整个集合物化成 `Vec`,内存是文档总字节的 **10–12 倍**。50 万条时单次调用会让 RSS 涨 **819 MiB**。

**新代码请用 `find` / `find_page` / `count`**,它们全部流式处理,内存 O(命中数)。`scan()` 已标注 deprecated。
:::

### 流式化之后

<PerfTable
  caption="find_page 内存增量 —— 两遍法:第一遍只收排序键,第二遍按主键回取"
  :columns="['规模', 'find_page 内存增量', '相比旧接口']"
  :rows="[
    ['100,000', { text: '12.2 MiB', num: true }, { text: '17× 改善', num: true, tone: 'good' }],
    ['500,000', { text: '30.2 MiB', num: true }, { text: '27× 改善', num: true, tone: 'good' }],
  ]"
/>

## 写入性能

### 三种写法的差距

<PerfTable
  caption="写入吞吐 —— 同一条文档,不同提交策略"
  :columns="['写法', 'macOS', 'Linux']"
  :rows="[
    ['逐条 insert(每条一个事务)', { text: '185 条/秒',   num: true, tone: 'bad' },  { text: '1,400 条/秒',   num: true, tone: 'bad' }],
    ['insert_many 批量',         { text: '44,000 条/秒', num: true, tone: 'good' }, { text: '300,000 条/秒', num: true, tone: 'good' }],
    ['提升',                     { text: '238×', num: true }, { text: '214×', num: true }],
  ]"
/>

::: danger 这不是实现缺陷,是硬件的物理成本
实测归因:逐条插入的耗时 ≈ **一次 fsync 的物理成本**。

<PerfTable
  caption="A/D 比值接近 1 —— 我们和裸 fsync 一样快,换引擎不会变快"
  :columns="['', '1,000 条', '10,000 条']"
  :rows="[
    ['A. nodblite 逐条插入', { text: '4.187 s', num: true }, { text: '228.9 s', num: true }],
    ['D. 裸 fsync 基准',     { text: '3.869 s', num: true }, { text: '302.0 s', num: true }],
    ['A / D',               { text: '1.08×',   num: true }, { text: '0.76×',   num: true }],
  ]"
/>
:::

### 而且会越写越慢

fsync 成本随数据文件增大而**恶化**:

<PerfTable
  caption="单次 fsync 随文件增长的成本"
  :columns="['文件规模', '单次 fsync']"
  :rows="[
    ['1,000 条',  { text: '3.87 ms',  num: true }],
    ['10,000 条', { text: '30.20 ms', num: true, tone: 'bad' }],
  ]"
/>

**8 倍的恶化**,导致逐条写入的吞吐从 238 条/秒掉到 43.7 条/秒。

::: tip 结论
**要吞吐就用 `insert_many`。** 把 N 次提交合并成 1 次,收益 2 个数量级。

超大批量导入还可以用流式批量会话(`nodblite_bulk_*` / JS 的 `bulkInsert()`),它把宿主侧的内存峰值从「整个数组」降到「单条文档」。
:::

## 索引

### 查询加速

<PerfTable
  caption="10 万文档,macOS,3 轮中位数"
  :columns="['查询场景', '全表扫', '走索引', '加速比']"
  :rows="[
    ['唯一字段等值(命中 1 条)',      { text: '510 ms', num: true }, { text: '1 ms',  num: true, tone: 'good' }, { text: '500×',  num: true, tone: 'good' }],
    ['范围(命中 1%,1,000 条)',       { text: '425 ms', num: true }, { text: '8 ms',  num: true, tone: 'good' }, { text: '55×',   num: true, tone: 'good' }],
    ['复合(前缀等值 + 后缀范围)',    { text: '410 ms', num: true }, { text: '22 ms', num: true, tone: 'good' }, { text: '19×',   num: true, tone: 'good' }],
    ['低选择率等值(命中 10%)',       { text: '430 ms', num: true }, { text: '76 ms', num: true },                { text: '5.7×',  num: true, tone: 'warn' }],
  ]"
/>

::: warning 最后一行值得注意
命中率高时索引收益急剧下降 —— 因为**回表成本占主导**。

规划器目前是**纯规则匹配**,没有区分度估算,所以低选择率场景下可能选择索引反而更慢(本例中仍然更快,但优势已很小)。这是下一步该做区分度估算的直接理由。
:::

### 索引的代价

<PerfTable
  caption="索引在同一个写事务内维护 —— 保证一致性,代价是写入变慢"
  :columns="['项', '数值']"
  :rows="[
    ['建索引回填 10 万条', { text: '1.2–2.5 s', num: true }],
    ['每多一个索引',       { text: '+80% 写入成本', num: true, tone: 'warn' }],
    ['3 个索引',           { text: '2.4× 写入成本', num: true, tone: 'bad' }],
  ]"
/>

::: tip 选型建议
只对**查询实际用到**的字段建索引。随便加索引会让写入明显变慢,而且低选择率字段的索引收益有限。
:::

::: details 一个已修复的缺陷:唯一索引建索引曾退化为 O(n²)
`create_index` 的唯一性前置检查原先用 `Vec::contains` 做线性查找。

- **症状**:10 万条唯一值建索引要 **74 秒**,而非唯一索引只要 2.6 秒(**28 倍**),且呈超线性恶化(1 万条 54 µs/条 → 10 万条 716 µs/条)
- **修复**:改用 `HashSet`,**74 s → 2.4 s(31 倍)**
- **回归**:新增 `unique_index_build_is_not_quadratic` 测试锁死

教训:**「两条路径必须等价」的对比测试价值远高于「功能能跑」的测试。**
:::

## 内存模型

<PerfTable
  caption="各操作的内存复杂度"
  :columns="['操作', '内存']"
  :rows="[
    ['find(无排序)',                          'O(命中数)'],
    ['find(带过滤,命中少)',                    'O(命中数)'],
    ['find_page(排序 + 分页)',                 'O(n × 排序键大小)'],
    ['get_by_id / update_by_id / delete_by_id', 'O(1)'],
    ['count',                                  'O(1)'],
    ['游标 ScanSession',                        'O(一批)'],
  ]"
/>

::: info 排序为什么是 O(n × 排序键)
`find_page` 用**两遍法**:

1. 第一遍流式扫描,只收集 `(排序键, _id)` —— 50 万条 × 32 字节 ≈ 16 MB
2. 排序后,按主键回取页内的 20 条

所以内存与**命中文档大小无关**,只与排序键大小有关。这就是为什么 50 万条时内存只有 30 MiB 而不是 800 MiB。
:::

## 并发

<PerfTable
  caption="160 条文档,8 线程"
  :columns="['场景', '吞吐', '加速比']"
  :rows="[
    ['单线程写入', { text: '92.7 条/秒',  num: true }],
    ['8 线程写入', { text: '139.0 条/秒', num: true }],
    ['加速比',     { text: '1.50×',       num: true, tone: 'warn' }],
  ]"
/>

::: note 为什么并发写入没有线性扩展
B+ 树是**单写者模型** —— 写事务被串行化。这是设计取舍,不是缺陷。

**并发对读有意义,对写意义有限。** 需要写吞吐请用批量写入,而不是加线程。
:::

## WASM

::: tip 结论:wasm 的读几乎无损,写与原生等价
同一份源码编译到 `wasm32-wasip1`,单文件 **774 KB**(gzip)覆盖所有平台。

<PerfTable
  caption="20,000 条,原生 vs wasm(交替 21 轮,取 min 作判据)"
  :columns="['指标', '原生', 'wasm', '比值']"
  :rows="[
    ['读取 5,000 条',        { text: '0.940 s', num: true }, { text: '0.949 s', num: true }, { text: '1.01×', num: true, tone: 'good' }],
    ['写入 20,000 条',       { text: '213 ms',  num: true }, { text: '277 ms',  num: true }, { text: '1.30×', num: true, tone: 'good' }],
    ['单次 fd_sync(9 MB)',   { text: '21.9 ms', num: true }, { text: '22.5 ms', num: true }, { text: '1.03×', num: true, tone: 'good' }],
  ]"
/>

**「wasm 写慢 3.6 倍」是测量假象。** fsync 本身有 54 倍方差(21 ms 到 1140 ms),单次或中位数测量都会被系统负载污染。改用 min 判据后,wasm 与原生几乎没有差别。
:::

::: details 为什么 wasm 的写不慢:瓶颈不在我们可控的层面
对 20,000 条写入做 WASI 调用计量:

<PerfTable
  :columns="['WASI 调用', '次数', '耗时', '占比']"
  :rows="[
    ['fd_pwrite', { text: '2,241', num: true }, { text: '20 ms',    num: true }, { text: '1.9%',  num: true }],
    ['fd_sync',   { text: '8',     num: true }, { text: '1,057 ms', num: true }, { text: '60–90%', num: true, tone: 'warn' }],
    ['其余',      { text: '—',     num: true }, { text: '6 ms',     num: true }, { text: '<1%',   num: true }],
  ]"
/>

**页写合并(把 2,241 次 pwrite 合并)最多只能省 1.9%** —— 所以我们没有做。

真正的成本是 fsync,而 fsync 的耗时主要来自**文件增长**(文件系统要为新增块分配并落元数据):

<PerfTable
  :columns="['场景', 'write', 'fsync']"
  :rows="[
    ['边写边增长 16 MB',    { text: '65 ms', num: true }, { text: '930 ms', num: true, tone: 'bad' }],
    ['预分配 16 MB 后写',   { text: '30 ms', num: true }, { text: '26 ms',  num: true, tone: 'good' }],
  ]"
/>

原生同样如此(9 MB 增长式 fsync = 720 ms)。**这是文件系统语义,不是 nodblite 的缺陷,也不是 wasm 的缺陷。**
:::

## 平台差异

<PerfTable
  caption="10k 档,同一份代码在两个平台"
  :columns="['指标', 'macOS', 'Linux']"
  :rows="[
    ['逐条插入',                  { text: '185 条/秒', num: true }, { text: '1,382 条/秒', num: true }],
    ['Rust sync_all()',           { text: '4.80 ms',  num: true }, { text: '0.37 ms',   num: true }],
    ['POSIX fsync()',             { text: '0.072 ms', num: true }, { text: '0.38 ms',   num: true }],
    ['sync_all / fsync 放大',     { text: '66.4×',    num: true, tone: 'bad' }, { text: '1.0×', num: true, tone: 'good' }],
    ['find 全量',                 { text: '8.25 µs/条', num: true }, { text: '1.19 µs/条', num: true }],
    ['update_by_id @100k',        { text: '20.7 ms',  num: true }, { text: '0.82 ms',   num: true }],
  ]"
/>

::: danger macOS 的 66 倍放大
Rust std 在 macOS/iOS 把 `sync_all()` 映射到 **`F_FULLFSYNC`**(强制磁盘写缓存落物理介质),代价是普通 `fsync` 的 66 倍。

Linux 映射到 `fdatasync`,没有这个放大。

**所以 macOS 上的 185 条/秒是最坏情况,不是引擎的普遍性能。**
:::

## 复现

所有数字都可以自己跑出来:

```bash
# 读路径:耗时与内存
cargo run --release --example bench -- 1000 10000 100000

# 写路径:四组对照(逐条 / 批量 / 非持久 / 裸 fsync)
cargo run --release --example bench_write -- 10000 3

# 索引:查询加速比与建索引代价
cargo run --release --example bench_index -- 100000

# wasm 对照:原生侧
cargo run --release --example bench_wasm_compare -- rows.json out.nodb 5

# wasm 对照:wasm 侧 + WASI 调用计量
node web/bench-write.mjs --wasm target/wasm32-wasip1/release/nodblite.wasm --n 20000
```
