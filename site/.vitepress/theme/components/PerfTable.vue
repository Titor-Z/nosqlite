<script setup>
/**
 * 性能表格:统一「数值右对齐 + 等宽数字 + 关键项标记」。
 *
 * 数据来自 docs/reference/performance.md 的实测结果,不在这里另存一份 ——
 * 通过 rows 传入,保证页面上只有一处真相。
 *
 * 为什么要组件而不是裸 Markdown 表格:
 *   - 数字要等宽对齐(裸表格做不到);
 *   - 好坏标记(加速比)需要按阈值着色,而不是手工标色;
 *   - 列宽要按内容分配,而不是均分。
 */
const props = defineProps({
  /** 表头 */
  columns: { type: Array, required: true },
  /** 行数据;每个单元格是 string 或 { text, num?, tone? } */
  rows: { type: Array, required: true },
  /** 说明文字 */
  caption: { type: String, default: '' },
})

function cell(v) {
  return typeof v === 'object' && v !== null ? v : { text: v }
}
</script>

<template>
  <figure class="nb-perf">
    <figcaption v-if="caption">{{ caption }}</figcaption>
    <div class="nb-perf-scroll">
      <table>
        <thead>
          <tr>
            <th v-for="c in columns" :key="c">{{ c }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, i) in rows" :key="i">
            <td v-for="(raw, j) in row" :key="j" :class="{ num: cell(raw).num }">
              <span :class="cell(raw).tone ? `tone-${cell(raw).tone}` : ''">
                {{ cell(raw).text }}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </figure>
</template>

<style scoped>
.nb-perf {
  margin: 1.5rem 0;
}

.nb-perf figcaption {
  font-size: 0.78rem;
  letter-spacing: 0.04em;
  color: var(--nb-text-faint);
  margin-bottom: 0.6rem;
}

/* 表格可以横向滚动,而不是压缩列宽 —— 数字挤在一起就失去可读性 */
.nb-perf-scroll {
  overflow-x: auto;
  border: 1px solid var(--nb-border);
  border-radius: var(--nb-radius-md);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.88rem;
  margin: 0;
}

th {
  text-align: left;
  font-size: 0.74rem;
  font-weight: 500;
  letter-spacing: 0.05em;
  color: var(--nb-text-muted);
  padding: 0.65rem 0.9rem;
  border-bottom: 1px solid var(--nb-border-strong);
  white-space: nowrap;
}

td {
  padding: 0.6rem 0.9rem;
  border-bottom: 1px solid var(--nb-border);
  white-space: nowrap;
}

tr:last-child td {
  border-bottom: none;
}

/* 数值列:等宽 + 右对齐,纵向可比 */
td.num {
  text-align: right;
  font-family: var(--vp-font-family-mono);
  font-variant-numeric: tabular-nums;
}

.tone-good {
  color: var(--nb-success);
}

.tone-bad {
  color: var(--nb-danger);
}

.tone-warn {
  color: var(--nb-warning);
}
</style>
