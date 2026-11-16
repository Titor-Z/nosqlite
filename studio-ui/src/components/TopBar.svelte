<script>
  import { onMount } from 'svelte';
  import { app, importFile, newDatabase, exportDatabase, bytesLabel } from '../lib/store.svelte.js';

  let input = $state(null);

  // 主题三态循环:跟随系统 → 亮 → 暗。localStorage 与首屏脚本共用同一个键。
  const THEMES = ['system', 'light', 'dark'];
  const THEME_LABEL = { system: '跟随系统', light: '亮色', dark: '暗色' };
  let theme = $state('system');
  let systemDark = $state(false);

  function apply(next) {
    theme = next;
    systemDark = matchMedia('(prefers-color-scheme: dark)').matches;
    const dark = next === 'dark' || (next === 'system' && systemDark);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    localStorage.setItem('nodblite.theme', next);
  }

  function cycleTheme() {
    apply(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]);
  }

  onMount(() => {
    theme = localStorage.getItem('nodblite.theme') || 'system';
    const media = matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => {
      if (theme === 'system') apply('system');
    };
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  });

  function pick(event) {
    const file = event.target.files?.[0];
    if (file) importFile(file);
    event.target.value = '';
  }
</script>

<header class="flex h-16 shrink-0 items-center gap-4 border-b border-line bg-panel/80 px-5">
  <div class="flex items-baseline gap-3">
    <span class="text-cyan glow text-lg leading-none">◈</span>
    <span class="text-[15px] font-bold tracking-[0.28em] text-text">NODBLITE</span>
    <span class="text-[10px] tracking-[0.32em] text-dim">// STUDIO</span>
  </div>

  <div class="mx-2 h-8 w-px bg-line"></div>

  <div class="min-w-0 flex-1">
    {#if app.opened}
      <div class="flex items-center gap-2 text-[12px]">
        <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-mint pulse-dot"></span>
        <span class="truncate text-text">{app.fileName}</span>
        <span class="tag tabular">{bytesLabel(app.fileSize)}</span>
      </div>
    {:else if app.ready}
      <div class="text-[10px] tracking-[0.22em] text-dim">NO DATABASE LOADED</div>
    {:else}
      <div class="text-[10px] tracking-[0.22em] text-amber">INITIALIZING WASM ENGINE…</div>
    {/if}
  </div>

  <div class="flex items-center gap-2">
    <button
      class="btn btn-ghost"
      onclick={cycleTheme}
      title="主题:{THEME_LABEL[theme]}(点击切换)"
      aria-label="切换主题:{THEME_LABEL[theme]}"
    >
      {#if theme === 'system'}
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-width="1.6" />
          <path d="M8 1v14" stroke="currentColor" stroke-width="1.6" />
          <path d="M8 3.5A4.5 4.5 0 0 1 8 12.5z" fill="currentColor" />
        </svg>
      {:else if theme === 'light'}
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <circle cx="8" cy="8" r="3.2" stroke="currentColor" stroke-width="1.6" />
          <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6L13 13M13 3l-1.4 1.4M4.4 11.6L3 13"
            stroke="currentColor" stroke-width="1.6" />
        </svg>
      {:else}
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M13.5 10.2A6 6 0 0 1 5.8 2.5a6 6 0 1 0 7.7 7.7z" stroke="currentColor" stroke-width="1.6" />
        </svg>
      {/if}
    </button>
    <button class="btn" disabled={!app.ready} onclick={() => input.click()}>载入 .nodb</button>
    <button class="btn" disabled={!app.ready} onclick={newDatabase}>新建库</button>
    <button class="btn btn-primary" disabled={!app.opened} onclick={exportDatabase}>导出</button>
    <input bind:this={input} type="file" accept=".nodb" class="hidden" onchange={pick} />
  </div>
</header>
