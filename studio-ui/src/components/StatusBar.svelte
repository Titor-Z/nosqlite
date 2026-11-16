<script>
  import { onMount } from 'svelte';
  import { app, bytesLabel } from '../lib/store.svelte.js';

  let clock = $state('--:--:--');

  onMount(() => {
    const tick = () => {
      clock = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  });

  const latest = $derived(app.logs[0] ?? '');
</script>

<footer class="flex h-8 shrink-0 items-center gap-3 border-t border-line bg-panel/80 px-4 text-[10px]">
  <span class="flex items-center gap-1.5">
    <span
      class="h-1.5 w-1.5 rounded-full {app.ready && !app.busy
        ? 'bg-mint pulse-dot'
        : 'bg-amber'}"
    ></span>
    <span class="tracking-[0.16em] {app.ready && !app.busy ? 'text-mint' : 'text-amber'}">
      {app.busy ? 'BUSY' : app.ready ? 'READY' : 'BOOT'}
    </span>
  </span>
  <span class="h-3 w-px bg-line"></span>
  <span class="truncate text-dim">{latest}</span>
  <span class="ml-auto flex items-center gap-3 text-dim">
    {#if app.opened}
      <span class="tabular">{app.collections.length} coll</span>
      <span class="tabular">{bytesLabel(app.fileSize)}</span>
    {/if}
    <span class="tabular text-cyan">{clock}</span>
  </span>
</footer>
