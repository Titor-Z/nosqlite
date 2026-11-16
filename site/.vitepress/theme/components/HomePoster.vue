<script setup>
/**
 * 首页:苏联构成主义海报。
 *
 * 版式不是「卡片列表」而是海报:
 *   - 一根斜向红条横切画面上缘,重复标语;
 *   - 巨型压缩标题 + 红底错位小标签;
 *   - 右侧硬框里放物理构图(文档 → 索引 → 单文件),当作海报的插图;
 *   - 数字条、规格网格、红黑对置的「用 / 不用」,最后落到墨底 CTA。
 *
 * 该页自带导航与页脚(layout: false),所以配色全部用组件内令牌,
 * 不跟随站点暗色主题 —— 海报就该固定纸感,不随系统切换。
 */
import { ref, onMounted } from 'vue'
import HeroPhysics from './HeroPhysics.vue'

const base = import.meta.env.BASE_URL

// 版本号与页脚信息是首页唯一需要手改的地方
const version = 'v0.6.11'

// 主题:与 VitePress 共用同一个 localStorage key,首页与文档页的状态互不打架。
// 初始值不自己猜,直接读 VitePress 内联脚本已经写在 <html> 上的 class。
const APPEARANCE_KEY = 'vitepress-theme-appearance'
const isDark = ref(false)

onMounted(() => {
  isDark.value = document.documentElement.classList.contains('dark')
})

const toggleTheme = () => {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  try {
    localStorage.setItem(APPEARANCE_KEY, isDark.value ? 'dark' : 'light')
  } catch {
    /* 隐私模式下写不进去也不影响本次切换 */
  }
}

// 窄屏只留三项,其余靠文档内的导航抵达 —— 挤成两行会拆开中文字
const nav = [
  { text: '指南', href: `${base}quickstart/rust` },
  { text: '使用', href: `${base}guide/query`, wide: true },
  { text: '性能', href: `${base}reference/performance` },
  { text: '兼容性', href: `${base}reference/compatibility`, wide: true },
  { text: 'FAQ', href: `${base}reference/faq`, wide: true },
  // Playground 是纯静态目录(不是 markdown 页),必须原生跳转
  { text: 'Playground', href: `${base}playground/` },
]

const stats = [
  { value: '7.98', unit: 'ms', label: '全量扫描 1,000 条' },
  { value: '44,000', unit: '条/秒', label: '批量写入(macOS)' },
  { value: '500', unit: '×', label: '唯一索引等值查询加速' },
  { value: '30.2', unit: 'MiB', label: '50 万条分页内存' },
]

const specs = [
  {
    no: '01',
    title: '单文件',
    text: '整个数据库就是一个 .nodb 文件。cp 即备份,拖进邮件即迁移。已在自动化测试中固化跨版本可读。',
  },
  {
    no: '02',
    title: 'MongoDB 语义',
    text: 'BSON 类型序、查询操作符、更新操作符、二级索引、游标协议 —— 按 MongoDB 实现并逐条测试过。',
  },
  {
    no: '03',
    title: '零配置',
    text: '没有服务进程、没有端口、没有配置文件。库直接跑在你的进程里。',
  },
  {
    no: '04',
    title: '四种接入形态',
    text: 'Rust 内嵌、C ABI、npm 包、wasm 单文件,以及让现有 MongoDB 驱动直连的 wire 协议。',
  },
  {
    no: '05',
    title: '真实驱动验证',
    text: 'Go 官方驱动 18 项、Node 官方驱动 12 项端到端测试全部通过 —— 不是自己造客户端测自己。',
  },
  {
    no: '06',
    title: '诚实的性能报告',
    text: '每个数字都标注平台与统计口径。macOS 的 fsync 放大 66 倍这类事实,我们写进文档而不是藏起来。',
  },
]

const fit = ['桌面 / 单机应用', '嵌入式后端(Rust / C)', '原型与工具链', '需要单文件迁移']
const unfit = ['多进程并发写同一个库', 'TB 级数据 / 高吞吐集群', '副本集、分片、认证', '重度依赖聚合管道']
</script>

<template>
  <div class="nb-poster" :class="{ dark: isDark }">
    <!-- 顶部导航:自建,layout:false 不带 VitePress 的导航 -->
    <header class="pnav">
      <a class="brand" :href="base">
        <span class="mark" aria-hidden="true">
          <span class="m1"></span><span class="m2"></span>
        </span>
        <span class="word">nodblite</span>
      </a>
      <nav class="links">
        <a v-for="n in nav" :key="n.text" :href="n.href" :class="{ wide: n.wide }">{{ n.text }}</a>
        <a href="https://github.com/Titor-Z/nosqlite" target="_blank" rel="noreferrer">GitHub</a>
        <button class="themebtn" type="button" :aria-pressed="isDark" aria-label="切换明暗主题" @click="toggleTheme">
          <svg v-if="!isDark" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z" />
          </svg>
          <svg v-else viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="4.2" />
            <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4" />
          </svg>
        </button>
      </nav>
    </header>

    <!-- 斜向红条:构成主义海报的签名元素,靠旋转与裁切出画面张力 -->
    <div class="band" aria-hidden="true">
      <span v-for="i in 6" :key="i">单文件 · 零配置 · 进程内 · MongoDB 语义 ·</span>
    </div>

    <section class="hero">
      <div class="copy">
        <h1>NODBLITE</h1>
        <div class="chiprow">
          <span class="chip">单文件 NoSQL 数据库</span>
        </div>
        <p class="lede">
          部署形态学 SQLite,数据模型学 MongoDB。<br />
          一个 <code>.nodb</code> 文件带走全部数据。
        </p>
        <div class="acts">
          <a class="btn solid" :href="`${base}quickstart/rust`">5 分钟上手</a>
          <a class="btn line" :href="`${base}reference/performance`">性能报告</a>
          <a class="btn line" :href="`${base}playground/`">在浏览器里试试</a>
        </div>
        <div class="meta">{{ version }} · Apache-2.0 · 531 例测试</div>
      </div>

      <!-- 海报插图:物理构图框(文档块 → 索引条 → 容器成形) -->
      <figure class="art nb-poster-hero" :data-scheme="isDark ? 'dark' : 'light'">
        <!-- 构图配色在初始化时确定,主题切换用 key 强制重建 -->
        <HeroPhysics :key="isDark ? 'dark' : 'light'" />
        <figcaption>
          <span>图 01</span>
          <span>文档 → 索引 → 一个文件</span>
        </figcaption>
      </figure>
    </section>

    <section class="numbers">
      <div v-for="s in stats" :key="s.label" class="num">
        <div class="v">{{ s.value }}<em>{{ s.unit }}</em></div>
        <div class="l">{{ s.label }}</div>
      </div>
    </section>

    <section class="specs">
      <article v-for="s in specs" :key="s.no" class="spec">
        <div class="no">{{ s.no }}</div>
        <h2>{{ s.title }}</h2>
        <p>{{ s.text }}</p>
      </article>
    </section>

    <section class="fit">
      <div class="fitcol yes">
        <h2>用它</h2>
        <ul>
          <li v-for="f in fit" :key="f">{{ f }}</li>
        </ul>
      </div>
      <div class="fitcol no">
        <h2>别用它</h2>
        <ul>
          <li v-for="f in unfit" :key="f">{{ f }}</li>
        </ul>
      </div>
    </section>

    <section class="cta">
      <div class="ctatext">
        <p class="slab">ONE FILE.<br />NO SERVER.</p>
        <p class="sub">单文件、零配置、进程内。复制 .nodb 即完成备份与迁移。</p>
      </div>
      <div class="ctaacts">
        <a class="btn solid-inv" :href="`${base}quickstart/rust`">5 分钟上手</a>
        <a class="btn line-inv" :href="`${base}reference/compatibility`">与 MongoDB 的差异</a>
      </div>
    </section>

    <footer class="pfoot">
      <span>nodblite · Apache-2.0</span>
      <span class="links">
        <a :href="`${base}quickstart/cli`">命令行</a>
        <a :href="`${base}quickstart/c-api`">C ABI</a>
        <a :href="`${base}quickstart/javascript`">npm</a>
        <a :href="`${base}quickstart/wire`">wire 协议</a>
      </span>
    </footer>
  </div>
</template>

<style scoped>
/* ==================== 局部令牌 ==================== */

.nb-poster {
  --paper: #efe6d4;
  --paper-2: #f7f1e4;
  --ink: #14110d;
  --red: #d0021b;
  --mono: 'JetBrains Mono', ui-monospace, Menlo, monospace;
  --display: 'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif;

  /* 半透明三档:暗色下必须整体反转,不能写死 rgb(20 17 13 / x) */
  --dim: rgb(20 17 13 / 0.62);
  --dim-2: rgb(20 17 13 / 0.55);
  --rule: rgb(20 17 13 / 0.1);

  background: var(--paper);
  color: var(--ink);
  font-family: 'Inter', 'Noto Sans SC', system-ui, sans-serif;
  overflow-x: hidden;
}

/* 暗色:墨底 + 纸线,朱红提亮一档以保证在深底上可读 */
.nb-poster.dark {
  --paper: #14110d;
  --paper-2: #1b1916;
  --ink: #f2ede3;
  --red: #e8402b;

  --dim: rgb(242 237 227 / 0.66);
  --dim-2: rgb(242 237 227 / 0.55);
  --rule: rgb(242 237 227 / 0.1);

  color-scheme: dark;
}

a {
  color: inherit;
  text-decoration: none;
}

/* ==================== 导航 ==================== */

.pnav {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.7rem clamp(1rem, 4vw, 3.5rem);
  border-bottom: 2px solid var(--ink);
}

.brand {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  font-family: var(--display);
  font-weight: 700;
  font-size: 1.05rem;
  letter-spacing: -0.02em;
}

/* 方块拼出的标识:墨块 + 朱红块,和 favicon 同构 */
.mark {
  display: inline-flex;
  gap: 2px;
}

.mark span {
  display: block;
  width: 12px;
  height: 22px;
}

.m1 {
  background: var(--ink);
}

.m2 {
  background: var(--red);
  height: 22px;
  clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
}

.links {
  display: flex;
  gap: clamp(0.7rem, 2vw, 1.6rem);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.18em;
  text-transform: uppercase;
}

.links a:hover {
  color: var(--red);
}

.themebtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  padding: 0;
  color: var(--ink);
  background: transparent;
  border: 2px solid var(--ink);
  cursor: pointer;
  transition:
    background 140ms ease,
    color 140ms ease;
}

.themebtn svg {
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: square;
}

.themebtn:hover {
  background: var(--ink);
  color: var(--paper);
}

/* ==================== 斜向标语带 ==================== */

.band {
  position: relative;
  display: flex;
  gap: 2.5rem;
  width: 108%;
  margin: 1.6rem 0 0 -4%;
  padding: 0.5rem 0;
  background: var(--red);
  color: var(--paper-2);
  border-top: 2px solid var(--ink);
  border-bottom: 2px solid var(--ink);
  transform: rotate(-2.2deg);
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  white-space: nowrap;
  overflow: hidden;
}

/* ==================== 主视觉 ==================== */

.hero {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(0, 0.95fr);
  gap: clamp(1.5rem, 4vw, 3.5rem);
  align-items: end;
  padding: clamp(2rem, 5vw, 4.5rem) clamp(1rem, 4vw, 3.5rem) clamp(1.5rem, 3vw, 2.5rem);
}

h1 {
  margin: 0;
  font-family: var(--display);
  font-size: clamp(3.2rem, 10vw, 7.4rem);
  font-weight: 700;
  line-height: 0.86;
  letter-spacing: -0.045em;
  text-transform: uppercase;
}

/* 红底错位标签:向左溢出,压住标题下沿 */
.chiprow {
  margin: 1.1rem 0 0 -0.35rem;
}

.chip {
  display: inline-block;
  background: var(--red);
  color: var(--paper-2);
  padding: 0.35rem 0.8rem;
  font-size: clamp(0.9rem, 1.6vw, 1.1rem);
  font-weight: 700;
  letter-spacing: 0.02em;
  transform: rotate(-0.8deg);
}

.lede {
  margin: 1.4rem 0 0;
  max-width: 34ch;
  font-size: clamp(0.95rem, 1.5vw, 1.05rem);
  line-height: 1.7;
}

.lede code {
  font-family: var(--mono);
  font-size: 0.92em;
  padding: 0.05em 0.3em;
  box-shadow: inset 0 0 0 1px var(--dim-2);
}

.acts {
  display: flex;
  flex-wrap: wrap;
  gap: 0.7rem;
  margin-top: 1.6rem;
}

.btn {
  display: inline-block;
  padding: 0.7rem 1.2rem;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.06em;
  border: 2px solid var(--ink);
  transition:
    background 140ms ease,
    color 140ms ease;
}

.btn.solid {
  background: var(--red);
  color: var(--paper-2);
  border-color: var(--red);
}

.btn.solid:hover {
  background: var(--ink);
  border-color: var(--ink);
}

.btn.line:hover {
  background: var(--ink);
  color: var(--paper-2);
}

.meta {
  margin-top: 1.2rem;
  font-family: var(--mono);
  font-size: 10.5px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--dim-2);
}

/* ==================== 插图框 ==================== */

.art {
  position: relative;
  margin: 0;
  aspect-ratio: 4 / 3.1;
  border: 2px solid var(--ink);
  background: var(--paper-2);
  overflow: hidden;
  align-self: end;
}

.art::before {
  /* 框内左上角的细网格,只作构造参考,不铺满 */
  content: '';
  position: absolute;
  inset: 0;
  background-image:
    linear-gradient(to right, var(--rule) 1px, transparent 1px),
    linear-gradient(to bottom, var(--rule) 1px, transparent 1px);
  background-size: 24px 24px;
  pointer-events: none;
  z-index: 1;
}

.art figcaption {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 2;
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.45rem 0.7rem;
  border-top: 2px solid var(--ink);
  background: var(--paper);
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

/* ==================== 数字条 ==================== */

.numbers {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  border-top: 2px solid var(--ink);
  border-bottom: 2px solid var(--ink);
  margin-top: clamp(1rem, 3vw, 2rem);
}

.num {
  padding: 1.2rem clamp(0.8rem, 2vw, 1.6rem);
  border-left: 1px solid var(--ink);
}

.num:first-child {
  border-left: none;
}

.num .v {
  font-family: var(--mono);
  font-size: clamp(1.5rem, 3.4vw, 2.4rem);
  font-weight: 500;
  letter-spacing: -0.04em;
  font-variant-numeric: tabular-nums;
  line-height: 1;
}

.num .v em {
  font-style: normal;
  font-size: 0.45em;
  margin-left: 0.3rem;
  letter-spacing: 0;
  color: var(--dim);
}

.num .l {
  margin-top: 0.6rem;
  font-size: 10.5px;
  letter-spacing: 0.13em;
  text-transform: uppercase;
  color: var(--dim);
}

/* ==================== 规格网格 ==================== */

.specs {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  border-bottom: 2px solid var(--ink);
}

.spec {
  padding: clamp(1.2rem, 2.6vw, 2rem) clamp(0.9rem, 2vw, 1.6rem);
  border-left: 1px solid var(--ink);
  border-top: 1px solid var(--ink);
}

.spec:nth-child(3n + 1) {
  border-left: none;
}

.spec:nth-child(-n + 3) {
  border-top: none;
}

.spec .no {
  font-family: var(--mono);
  font-size: 0.95rem;
  letter-spacing: 0.1em;
  color: var(--red);
}

.spec h2 {
  margin: 0.5rem 0 0;
  font-family: var(--display);
  font-size: 1.05rem;
  font-weight: 700;
  letter-spacing: -0.01em;
  border: none;
  padding: 0;
}

.spec p {
  margin: 0.55rem 0 0;
  font-size: 0.86rem;
  line-height: 1.65;
  color: var(--dim);
}

/* ==================== 用 / 不用:红黑对置 ==================== */

.fit {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  border-bottom: 2px solid var(--ink);
}

.fitcol {
  padding: clamp(1.4rem, 3vw, 2.4rem) clamp(1rem, 3vw, 2.4rem);
}

.fitcol.yes {
  background: var(--ink);
  color: var(--paper-2);
}

.fitcol.no {
  background: var(--red);
  color: var(--paper-2);
}

.fitcol h2 {
  margin: 0 0 1rem;
  font-family: var(--display);
  font-size: clamp(1.6rem, 3.4vw, 2.4rem);
  font-weight: 700;
  letter-spacing: -0.03em;
  border: none;
  padding: 0;
}

.fitcol ul {
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 0.9rem;
  line-height: 2;
}

.fitcol li {
  display: flex;
  gap: 0.6rem;
}

.fitcol li::before {
  content: '';
  flex: none;
  width: 10px;
  height: 3px;
  margin-top: 0.83em;
  background: currentColor;
  opacity: 0.85;
}

/* ==================== 结尾 CTA ==================== */

.cta {
  border-top: 2px solid var(--paper-2);
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 0.6fr);
  gap: 2rem;
  align-items: center;
  padding: clamp(2rem, 5vw, 4rem) clamp(1rem, 4vw, 3.5rem);
  background: var(--ink);
  color: var(--paper-2);
}

.slab {
  margin: 0;
  font-family: var(--display);
  font-size: clamp(2rem, 6vw, 4.2rem);
  font-weight: 700;
  line-height: 0.92;
  letter-spacing: -0.045em;
}

.sub {
  margin: 1rem 0 0;
  max-width: 40ch;
  font-size: 0.9rem;
  line-height: 1.7;
  opacity: 0.76;
}

.ctaacts {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  align-items: stretch;
}

.btn.solid-inv {
  background: var(--red);
  border-color: var(--red);
  color: var(--paper-2);
  text-align: center;
}

.btn.solid-inv:hover {
  background: var(--paper-2);
  border-color: var(--paper-2);
  color: var(--ink);
}

.btn.line-inv {
  border-color: var(--paper-2);
  color: var(--paper-2);
  text-align: center;
}

.btn.line-inv:hover {
  background: var(--paper-2);
  color: var(--ink);
}

/* ==================== 页脚 ==================== */

.pfoot {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  justify-content: space-between;
  padding: 1.1rem clamp(1rem, 4vw, 3.5rem);
  border-top: 2px solid var(--ink);
  font-family: var(--mono);
  font-size: 10.5px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.pfoot .links {
  display: flex;
  gap: 1.2rem;
}

.pfoot a:hover {
  color: var(--red);
}

/* ==================== 响应式 ==================== */

@media (max-width: 960px) {
  .hero {
    grid-template-columns: 1fr;
    align-items: start;
  }

  .art {
    aspect-ratio: 4 / 3.2;
  }

  .numbers {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .num:nth-child(3) {
    border-left: none;
  }

  .num:nth-child(n + 3) {
    border-top: 1px solid var(--ink);
  }

  .specs {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .spec:nth-child(3n + 1) {
    border-left: 1px solid var(--ink);
  }

  .spec:nth-child(2n + 1) {
    border-left: none;
  }

  .spec:nth-child(-n + 3) {
    border-top: 1px solid var(--ink);
  }

  .spec:nth-child(-n + 2) {
    border-top: none;
  }

  .fit {
    grid-template-columns: 1fr;
  }

  .cta {
    grid-template-columns: 1fr;
  }

  .ctaacts {
    flex-direction: row;
    flex-wrap: wrap;
  }
}

@media (max-width: 720px) {
  .links a.wide {
    display: none;
  }

  .band {
    font-size: 10px;
    letter-spacing: 0.16em;
  }
}

@media (max-width: 560px) {
  .numbers {
    grid-template-columns: 1fr;
  }

  .num {
    border-left: none;
    border-top: 1px solid var(--ink);
  }

  .num:first-child {
    border-top: none;
  }

  .specs {
    grid-template-columns: 1fr;
  }

  .spec {
    border-left: none !important;
    border-top: 1px solid var(--ink) !important;
  }

  .spec:first-child {
    border-top: none !important;
  }
}
</style>
