<script>
  import { app, closeToast } from '../lib/store.svelte.js';
</script>

{#if app.toast}
  <button
    class="fixed bottom-12 right-5 z-[80] max-w-[min(420px,90vw)] bg-panel shadow-hard border-2 border-ink px-4 py-3 text-left text-[12px] shadow-lg transition {app.toast.kind === 'ok'
      ? 'border-mint/60 text-mint'
      : 'border-danger/60 text-danger'}"
    onclick={closeToast}
  >
    <span class="mr-2">{app.toast.kind === 'ok' ? '✓' : '✕'}</span>
    {app.toast.text}
  </button>
{/if}
