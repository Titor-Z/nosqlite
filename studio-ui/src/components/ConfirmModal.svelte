<script>
  import { app, confirmYes, confirmNo } from '../lib/store.svelte.js';
</script>

{#if app.confirm}
  <div class="fixed inset-0 z-[75] flex items-center justify-center p-4">
    <button
      class="absolute inset-0 cursor-default bg-void/85 backdrop-blur-[2px]"
      aria-label="取消"
      onclick={confirmNo}
    ></button>
    <div class="panel relative z-10 w-[min(460px,92vw)] p-5">
      <div class="mb-2 flex items-center gap-2">
        <span class="text-amber">▲</span>
        <span class="text-[11px] tracking-[0.2em] text-text">{app.confirm.title}</span>
      </div>
      <p class="mb-5 text-[12px] leading-relaxed text-dim">{app.confirm.text}</p>
      <div class="flex justify-end gap-2">
        <button class="btn" onclick={confirmNo}>取消</button>
        <button
          class="btn {app.confirm.danger ? 'btn-danger' : 'btn-primary'}"
          onclick={confirmYes}
        >
          确认
        </button>
      </div>
    </div>
  </div>
{/if}
