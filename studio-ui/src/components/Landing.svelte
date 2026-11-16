<script>
  /**
   * Playground 入口(官网 /playground/ 与单二进制的落地页共用)。
   *
   * 版式沿用站点的苏联构成主义海报语言:斜向红条、巨型压缩标题、
   * 红底错位标签、硬框拖入区、纸/墨/朱红对置 —— 从官网点进来不该像换了产品。
   *
   * 三条必须说清的事实(不是营销,是安全提示):
   *   1. 引擎是 WebAssembly,在标签页内运行;
   *   2. 文件通过浏览器 File API 本地读取,不会上传;
   *   3. 数据库活在本页内存里,刷新即丢失 —— 要长期使用请下载单二进制。
   */
  import { app, importFile, newDatabase } from '../lib/store.svelte.js';

  const RELEASES = 'https://github.com/Titor-Z/nosqlite/releases';
  const DOCS = 'https://titor-z.github.io/nosqlite/';

  let input = $state(null);

  function pick(event) {
    const file = event.target.files?.[0];
    if (file) importFile(file);
    event.target.value = '';
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-auto">
  <!-- 斜向标语带:与官网首页同一条元素 -->
  <div class="shrink-0 overflow-hidden border-y-2 border-ink bg-cyan py-1">
    <div
      class="flex w-[108%] -translate-x-[3%] gap-10 whitespace-nowrap font-mono text-[10px] tracking-[0.22em] text-on-accent uppercase"
      style="transform: rotate(-2.2deg)"
      aria-hidden="true"
    >
      {#each Array(6) as _}
        <span>playground · wasm engine · your file never leaves this tab ·</span>
      {/each}
    </div>
  </div>

  <section class="grid shrink-0 gap-0 lg:grid-cols-[1.05fr_0.95fr]">
    <!-- 左:标题与说明 -->
    <div class="border-ink px-6 py-8 sm:px-10 sm:py-12 lg:border-r-2">
      <h1 class="font-sans text-[clamp(2.2rem,7vw,4.4rem)] leading-[0.88] font-bold tracking-[-0.045em] uppercase">
        Nodblite<br />Studio
      </h1>

      <div class="mt-4 ml-[-0.2rem] inline-block -rotate-[0.8deg] bg-cyan px-3 py-1 text-[13px] font-bold text-on-accent">
        浏览器内 playground
      </div>

      <p class="mt-5 max-w-[40ch] text-[13px] leading-relaxed text-text">
        把 <code class="border border-line px-1 font-mono text-[12px]">.nodb</code> 拖进来就能查看集合、文档、索引与查询计划 ——
        引擎是 WebAssembly,跑在这个标签页里。
      </p>

      <ul class="mt-6 max-w-[40ch] space-y-2 text-[12px] leading-relaxed text-dim">
        <li class="flex gap-2"><span class="mt-[0.55em] h-[3px] w-2.5 shrink-0 bg-current"></span>文件由浏览器本地读取,<span class="font-bold text-text">不会上传</span></li>
        <li class="flex gap-2"><span class="mt-[0.55em] h-[3px] w-2.5 shrink-0 bg-current"></span>数据库活在本页内存里,<span class="font-bold text-text">刷新即丢失</span></li>
        <li class="flex gap-2"><span class="mt-[0.55em] h-[3px] w-2.5 shrink-0 bg-current"></span>要长期使用,请下载单二进制版直接读写磁盘文件</li>
      </ul>

      <div class="mt-7 flex flex-wrap gap-2.5">
        <a class="btn btn-primary" href={RELEASES} target="_blank" rel="noreferrer">下载单二进制</a>
        <a class="btn btn-ghost" href={DOCS} target="_blank" rel="noreferrer">文档</a>
      </div>
    </div>

    <!-- 右:拖入区 -->
    <div class="flex items-center px-6 py-8 sm:px-10 sm:py-12">
      <div
        class="panel shadow-hard flex w-full flex-col items-center border-2 px-6 py-10 text-center"
      >
        <div class="label text-cyan">INGEST</div>

        <div class="mt-4 text-[20px] leading-tight font-bold tracking-[-0.01em] text-text">
          把 .nodb 拖到这里
        </div>
        <div class="mt-2 text-[11px] text-dim">或从磁盘选择。文件只在你的浏览器内打开。</div>

        <div class="mt-6 flex flex-wrap justify-center gap-2.5">
          <button class="btn btn-primary" onclick={() => input.click()}>选择文件</button>
          <button class="btn" onclick={newDatabase}>新建空数据库</button>
          <input bind:this={input} type="file" accept=".nodb" class="hidden" onchange={pick} />
        </div>

        <div class="mt-8 grid w-full grid-cols-3 gap-px border border-ink bg-line text-[10px]">
          <div class="bg-panel px-2 py-2.5">
            <div class="font-bold text-cyan">WASM</div>
            <div class="mt-1 text-dim">引擎在页面内</div>
          </div>
          <div class="bg-panel px-2 py-2.5">
            <div class="font-bold text-mint">LOCAL</div>
            <div class="mt-1 text-dim">数据不出本机</div>
          </div>
          <div class="bg-panel px-2 py-2.5">
            <div class="font-bold text-magenta">ONE FILE</div>
            <div class="mt-1 text-dim">.nodb 单文件</div>
          </div>
        </div>
      </div>
    </div>
  </section>

  <!-- 底部:两种形态的差异(引擎状态由底部 StatusBar 承担,不重复) -->
  <section class="mt-auto grid shrink-0 border-t-2 border-ink sm:grid-cols-2">
    <div class="border-ink px-6 py-4 sm:px-10 sm:border-r-2">
      <div class="label text-dim">浏览器版</div>
      <p class="mt-1 text-[11px] leading-relaxed text-dim">临时查看与演示,数据在本页内存,刷新即丢失。</p>
    </div>
    <div class="px-6 py-4 sm:px-10">
      <div class="label text-dim">单二进制版</div>
      <p class="mt-1 text-[11px] leading-relaxed text-dim">
        直接读写磁盘上的 <span class="font-mono">.nodb</span>,不需要浏览器,适合长期使用。
      </p>
    </div>
  </section>
</div>
