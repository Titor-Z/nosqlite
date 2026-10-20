---
layout: home

hero:
  name: nodblite
  text: 单文件 NoSQL 数据库
  tagline: 部署形态学 SQLite,数据模型学 MongoDB。一个 .nodb 文件带走全部数据。
  image:
    src: /logo.svg
    alt: nodblite
  actions:
    - theme: brand
      text: 5 分钟上手
      link: /quickstart/rust
    - theme: alt
      text: 性能报告
      link: /reference/performance
    - theme: alt
      text: GitHub
      link: https://github.com/Titor-Z/nosqlite

features:
  - title: 单文件
    details: 整个数据库就是一个 .nodb 文件。cp 即备份,拖进邮件即迁移。已在自动化测试中固化跨版本可读。
  - title: MongoDB 语义
    details: BSON 类型序、查询操作符、更新操作符、二级索引、游标协议 —— 按 MongoDB 实现并逐条测试过。
  - title: 零配置
    details: 没有服务进程、没有端口、没有配置文件。库直接跑在你的进程里。
  - title: 四种接入形态
    details: Rust 内嵌、C ABI、npm 包、wasm 单文件,以及让现有 MongoDB 驱动直连的 wire 协议。
  - title: 真实驱动验证
    details: Go 官方驱动 18 项、Node 官方驱动 12 项端到端测试全部通过 —— 不是自己造客户端测自己。
  - title: 诚实的性能报告
    details: 每个数字都标注平台与统计口径。macOS 的 fsync 放大 66 倍这类事实,我们写进文档而不是藏起来。
---

<script setup>
import { ref, onMounted } from 'vue'

// 首页统计条。数字全部来自 docs/reference/performance.md 的实测。
const stats = ref([
  { value: '7.98', unit: 'ms', label: '全量扫描 1,000 条' },
  { value: '44,000', unit: '条/秒', label: '批量写入(macOS)' },
  { value: '500', unit: '×', label: '唯一索引等值查询加速' },
  { value: '30.2', unit: 'MiB', label: '50 万条分页内存' },
])
</script>

<div class="nb-hero-wrap">
  <HeroPhysics />
</div>

<div class="nb-stats">
  <div class="nb-stat" v-for="s in stats" :key="s.label">
    <div class="value">{{ s.value }}<span class="unit">{{ s.unit }}</span></div>
    <div class="label">{{ s.label }}</div>
  </div>
</div>

## 它是什么

nodblite 把 MongoDB 的文档语义装进一个像 SQLite 一样可直接拷贝的单文件里。

```bash
cargo add nodblite        # Rust
npm install nodblite      # Node
```

```rust
use nodblite::{Database, Document};
use nodblite::query::Filter;

let db = Database::create("app.nodb")?;

let mut doc = Document::from_json(r#"{"name": "张三", "age": 30}"#)?;
db.insert("users", &mut doc)?;

let adults = db.find("users", &Filter::from_value(
    &serde_json::json!({"age": {"$gte": 18}})
)?)?;
```

## 四种接入形态

| 形态 | 适用场景 | 入口 |
|---|---|---|
| **Rust 内嵌** | Rust 后端、CLI 工具 | `cargo add nodblite` |
| **C ABI** | C/C++/Python/Go/任何能调 C 的语言 | 下载 `nodblite-ffi-*` |
| **npm 包** | Node.js 应用 | `npm install nodblite` |
| **WASM** | 跨平台单文件、浏览器、边缘运行时 | 下载 `nodblite-wasm` |
| **wire 协议** | 让现有 MongoDB 驱动直连 | `nodblite-server` |

::: tip WASM:一份产物覆盖所有平台
`wasm32-wasip1` 编译出的单个 `.wasm` 只有 **774 KB**(gzip),却能跑在任何有 WASI 运行时的宿主上 —— 不必为每个平台各分发一份动态库。`.nodb` 文件与原生完全互通。
:::

## 何时该用

::: tip 适合
- **桌面 / 单机应用** —— 一个文件带走全部数据,不需要用户装数据库
- **嵌入式后端** —— Rust/C 服务内嵌,省掉一个数据库运维对象
- **原型与工具链** —— 想用 MongoDB 的查询手感,但不想跑 mongod
- **需要单文件迁移** —— 复制 `.nodb` 即完成备份/迁移
:::

::: danger 不适合
| 场景 | 原因 |
|---|---|
| **多进程并发写同一个库** | 单文件由进程独占,这是 SQLite 同款约束 |
| **TB 级数据 / 高写入吞吐集群** | 定位是嵌入式单机库,不是分布式系统 |
| **需要副本集、分片、认证** | wire 协议已实现,但这些能力不在范围内 |
| **重度依赖聚合管道** | 仅支持 7 个 stage |
| **跨集合事务** | 单语句原子 + 单事务批量,但没有显式事务会话 |
:::

## 迁移前必读

我们尽力对齐 MongoDB 语义,但有几处**刻意不同**,也有一批**确实还没做**。

- [与 MongoDB 的差异](/reference/compatibility) —— 类型系统、事务、并发模型
- [性能报告](/reference/performance) —— 每个数字都标注平台与口径
- [常见问题](/reference/faq)

<style>
/* 物理画布盖在 hero 区域上。
   layout:home 的内容槽在 hero 之后,把它绝对定位脱 离文档流、
   锚定到内容容器顶部(即 hero 顶部),后续内容不受影响。 */
/* 锚定:组件在 mounted 时会把自身 DOM 移到 .VPHero 里,
   所以这里只需要 hero 自身建立定位上下文(主题已有)。 */
.VPHero .nb-hero-physics {
  position: absolute;
  inset: 0;
  pointer-events: auto;
}

/* hero 的静态 logo 藏掉 —— 用物理画布替代它 */
.VPHero .VPImage {
  display: none;
}

/* hero 文本压在画布之上 */
.VPHero .container {
  position: relative;
  z-index: 1;
}

/* 画布下方的 features 区要能正常接收点击 */
.VPFeatures {
  position: relative;
  z-index: 2;
}

@media (max-width: 960px) {
  .VPHero .nb-hero-physics {
    opacity: 0.55;
  }
}

@media (prefers-reduced-motion: reduce) {
  .nb-hero-wrap {
    opacity: 0.3;
  }
}
</style>
