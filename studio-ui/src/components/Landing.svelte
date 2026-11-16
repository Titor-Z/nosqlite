<script>
  import { app, importFile, newDatabase } from '../lib/store.svelte.js';

  let input = $state(null);

  function pick(event) {
    const file = event.target.files?.[0];
    if (file) importFile(file);
    event.target.value = '';
  }
</script>

<section class="flex min-h-0 flex-1 items-center justify-center p-6">
  <div class="w-full max-w-3xl scan-reveal">
    <div class="mb-8 text-center">
      <div class="mb-3 text-[10px] tracking-[0.45em] text-dim">DOCUMENT STORAGE ACCESS TERMINAL</div>
      <h1 class="text-4xl font-bold tracking-[0.16em] text-text glow">NODBLITE STUDIO</h1>
      <p class="mt-4 text-[12px] leading-relaxed text-dim">
        单文件 NoSQL 数据库的浏览器控制台 · 引擎以 WebAssembly 在本页内运行
      </p>
    </div>

    <div
      class="panel shadow-hard-lg flex flex-col items-center px-10 py-14 text-center"
    >
      <div class="mb-6 flex items-center gap-3 text-[11px] tracking-[0.3em] text-cyan">
        <span class="inline-block h-px w-14 bg-cyan/60"></span>
        INGEST DATABASE
        <span class="inline-block h-px w-14 bg-cyan/60"></span>
      </div>

      <div class="mb-2 text-2xl tracking-[0.12em] text-text">把 .nodb 文件拖到这里</div>
      <div class="mb-8 text-[11px] text-dim">或从磁盘选择。文件只在你的浏览器内存中打开,不会上传。</div>

      <div class="flex flex-wrap justify-center gap-3">
        <button class="btn btn-primary" onclick={() => input.click()}>选择文件</button>
        <button class="btn" onclick={newDatabase}>新建空数据库</button>
        <input bind:this={input} type="file" accept=".nodb" class="hidden" onchange={pick} />
      </div>

      <div class="mt-10 grid w-full max-w-lg grid-cols-3 gap-px bg-line text-[10px]">
        <div class="bg-panel px-3 py-3">
          <div class="text-cyan">WASM</div>
          <div class="mt-1 text-dim">引擎在页面内</div>
        </div>
        <div class="bg-panel px-3 py-3">
          <div class="text-amber">LOCAL</div>
          <div class="mt-1 text-dim">数据不出本机</div>
        </div>
        <div class="bg-panel px-3 py-3">
          <div class="text-magenta">SINGLE FILE</div>
          <div class="mt-1 text-dim">.nodb 单文件</div>
        </div>
      </div>
    </div>

    <div class="mt-6 flex items-center justify-center gap-2 text-[10px] tracking-[0.18em] text-dim">
      <span class="h-1 w-1 rounded-full bg-mint"></span>
      {app.ready ? 'ENGINE READY' : 'BOOTING ENGINE'}
    </div>
  </div>
</section>
