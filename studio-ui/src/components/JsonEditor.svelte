<script>
  /**
   * 轻量 JSON 编辑器:透明 textarea 叠在高亮层之上。
   * 不引第三方依赖,只做词法着色;滚动位置与输入层同步。
   */
  let { value = $bindable(), height = '52vh', onkeydown = undefined } = $props();

  let pre = $state(null);
  let ta = $state(null);

  const tokens = $derived(highlight(value ?? ''));

  function esc(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  function plain(s) {
    return esc(s).replace(/([{}[\],])/g, '<span class="tok-punc">$1</span>');
  }

  function highlight(text) {
    const re = /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b/g;
    let out = '';
    let last = 0;
    let match;
    while ((match = re.exec(text))) {
      out += plain(text.slice(last, match.index));
      if (match[1]) {
        out += `<span class="tok-key">${esc(match[1])}</span>`;
        if (match[2]) out += `<span class="tok-punc">${esc(match[2])}</span>`;
      } else if (match[3]) {
        out += `<span class="tok-num">${match[3]}</span>`;
      } else {
        out += `<span class="tok-bool">${match[4]}</span>`;
      }
      last = re.lastIndex;
    }
    return out + plain(text.slice(last));
  }

  function syncScroll() {
    if (!pre || !ta) return;
    pre.scrollTop = ta.scrollTop;
    pre.scrollLeft = ta.scrollLeft;
  }
</script>

<div class="json-editor" style="height:{height}">
  <pre class="json-editor-pre" bind:this={pre} aria-hidden="true">{@html tokens}</pre>
  <textarea
    bind:this={ta}
    class="json-editor-input"
    bind:value
    spellcheck="false"
    autocapitalize="off"
    autocomplete="off"
    aria-label="JSON"
    onscroll={syncScroll}
    {onkeydown}
  ></textarea>
</div>
