<script setup>
/**
 * 首页 hero:文档块落进一个 .nodb 容器 —— 构成主义版。
 *
 * 为什么物理在这里成立:
 *   构成主义的语言是直角、轴对齐、栅格;物理不是装饰,而是把这套语言演出来
 *   (构造过程本身就是设计)。前提是约束刚体的形态:
 *   - 矩形板 + `inertia: Infinity` 禁止旋转 → 只允许平移,堆叠必然轴对齐;
 *   - 生成点按列对齐,块宽 = 列宽 - 间隙 → 块竖直落入本列,得到柱状栅格;
 *   - 高摩擦 + 近零弹性 → 落定即静止,不留残余抖动。
 * 三者缺一,物理就会退化成「一堆圆在乱滚」,与构成主义冲突。
 *
 * 叙事(分批落下):
 *   文档块 → 朱红索引条 → 容器框成形,左下状态行同步 01/02/03。
 *
 * 明确不做的事:
 *   - 不做鼠标拖拽:拖走一块就毁掉构图,严格栅格优先;
 *   - 不做落定后的取整吸附:取整会把相邻块挤到一起,列对齐已保证栅格;
 *   - 不阻塞 LCP(等 load + 一帧)、页面不可见暂停、reduced-motion 走静态终态。
 */
import { onMounted, onBeforeUnmount, ref } from 'vue'

const host = ref(null)

let cleanup = []
let lifecycle = 0

/**
 * 判断背景明暗,用于选择「墨底纸线 / 纸底墨线」。
 * 必须同时支持 #rrggbb 与 rgb()/rgba() —— 只认十进制数字的正则会把
 * `#f2ede3` 解析错(十六进制字母被当成分隔符),导致永远判定为深色。
 */
function isDarkColor(color) {
  let r, g, b
  const hex = color.replace('#', '')
  if (/^[0-9a-f]{6}$/i.test(hex)) {
    r = parseInt(hex.slice(0, 2), 16)
    g = parseInt(hex.slice(2, 4), 16)
    b = parseInt(hex.slice(4, 6), 16)
  } else if (/^[0-9a-f]{3}$/i.test(hex)) {
    r = parseInt(hex[0] + hex[0], 16)
    g = parseInt(hex[1] + hex[1], 16)
    b = parseInt(hex[2] + hex[2], 16)
  } else {
    const m = color.match(/(\d+(?:\.\d+)?)\D+(\d+(?:\.\d+)?)\D+(\d+(?:\.\d+)?)/)
    if (!m) return false
    ;[r, g, b] = [Number(m[1]), Number(m[2]), Number(m[3])]
  }
  return (r * 299 + g * 587 + b * 114) / 1000 < 128
}

async function init() {
  const el = host.value
  if (!el) return

  const myLifecycle = lifecycle
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

  // 等首屏文本渲染完再启动,避免抢占 LCP
  await new Promise((r) => (document.readyState === 'complete' ? r() : window.addEventListener('load', r, { once: true })))
  await new Promise((r) => requestAnimationFrame(r))
  if (myLifecycle !== lifecycle) return

  const Matter = (await import('matter-js')).default
  const { Engine, Runner, Bodies, Composite, Body } = Matter

  // 画布需要一个 relative 定位的宿主:
  //   - 海报首页(.nb-poster-hero):就地放,不必搬;
  //   - VitePress home(layout: home):本组件渲染在 markdown 内容槽,外面包着
  //     position:relative 的包装 div,必须把宿主 DOM 搬进 .VPHero 才能对齐。
  const anchor = el.closest('.nb-poster-hero') || document.querySelector('.VPHero')
  if (anchor && el.parentElement !== anchor) anchor.appendChild(el)

  const dpr = Math.min(window.devicePixelRatio || 1, 2)
  const width = el.clientWidth
  const height = el.clientHeight

  const canvas = document.createElement('canvas')
  canvas.width = width * dpr
  canvas.height = height * dpr
  canvas.style.width = '100%'
  canvas.style.height = '100%'
  canvas.style.display = 'block'
  canvas.setAttribute('aria-hidden', 'true')
  el.appendChild(canvas)

  const ctx = canvas.getContext('2d')
  ctx.scale(dpr, dpr)

  // 极窄容器里没有任何可读的构图空间,直接不画
  if (width < 200 || height < 140) {
    canvas.remove()
    return
  }

  const styles = getComputedStyle(document.documentElement)
  // 宿主可以强制明暗(海报页固定纸底,不跟随站点主题切换)
  const forced = el.closest('[data-scheme]')?.dataset.scheme
  const dark = forced ? forced === 'dark' : isDarkColor(styles.getPropertyValue('--nb-bg').trim() || '#08090a')
  const mono = styles.getPropertyValue('--vp-font-family-mono').trim() || 'monospace'

  // 构成主义令牌(与 studio 同源)
  const PAPER = '#fbf7ee'
  const INK = '#12100e'
  const ACCENT = '#d6281e'

  const blockFill = dark ? '#1b1916' : '#fffdf8'
  const blockLine = dark ? PAPER : INK
  const gridLine = dark ? 'rgba(251,247,238,0.14)' : 'rgba(18,16,14,0.14)'
  const faint = dark ? 'rgba(251,247,238,0.55)' : 'rgba(18,16,14,0.55)'

  /* ---------------- 布局:列(槽位) ---------------- */

  // 构图居中在宿主框内(海报插图 / hero 右侧区域都适用)
  const cols = width < 420 ? 2 : 3
  const boxW = Math.min(width * 0.72, 400)
  const boxH = Math.min(height * 0.66, 270)
  const boxX = (width - boxW) / 2
  const boxY = Math.max(28, (height - boxH) / 2 + 14)
  const colW = boxW / cols
  const GAP = 7
  const blockW = colW - GAP

  const colCenter = (i) => boxX + colW * (i + 0.5)

  /* ---------------- 物理 ---------------- */

  const engine = Engine.create({ enableSleeping: true, gravity: { x: 0, y: 0.9 } })
  const world = engine.world

  // 地板 + 两侧厚墙(薄墙会被高速块隧穿)
  const wall = 60
  const statics = [
    Bodies.rectangle(boxX + boxW / 2, boxY + boxH + wall / 2, boxW + wall * 2, wall, { isStatic: true }),
    Bodies.rectangle(boxX - wall / 2, boxY + boxH / 2, wall, boxH * 2, { isStatic: true }),
    Bodies.rectangle(boxX + boxW + wall / 2, boxY + boxH / 2, wall, boxH * 2, { isStatic: true }),
  ]
  Composite.add(world, statics)

  const blocks = [] // { body, kind: 'doc' | 'index', col }

  // 每列文档数不同 → 高低柱,构图有节奏
  const perCol = cols === 2 ? [4, 3] : [5, 3, 4]

  // 由文档块与索引条的先后顺序生成时间线
  const plan = []
  let order = 0
  const roundCount = Math.max(...perCol)
  for (let r = 0; r < roundCount; r++) {
    for (let c = 0; c < cols; c++) {
      if (r >= perCol[c]) continue
      plan.push({ kind: 'doc', col: c, at: 260 + order * 230 })
      order++
    }
  }
  // 索引条:落在最高那根柱顶,代表「索引建在文档之上」
  const tallest = perCol.indexOf(Math.max(...perCol))
  plan.push({ kind: 'index', col: tallest, at: 260 + order * 230 })
  const indexAt = plan[plan.length - 1].at
  const frameAt = indexAt + 520

  const spawn = ({ kind, col }) => {
    // 窄框只有 2 列,块更宽,相应加高才好读
    const h = kind === 'index' ? 5 : (20 + Math.random() * 16) * (cols === 2 ? 1.5 : 1)
    const body = Bodies.rectangle(colCenter(col), boxY - 24 - Math.random() * 40, blockW, h, {
      inertia: Infinity, // 禁止旋转:严格直角体系
      restitution: 0.02,
      friction: 0.9,
      frictionStatic: 1,
      frictionAir: 0.02,
      density: 0.002,
      chamfer: null,
    })
    Composite.add(world, body)
    blocks.push({ body, kind, col })
  }

  /* ---------------- 绘制 ---------------- */

  let frameAlpha = 0
  let elapsed = 0
  let stage = 0

  const draw = () => {
    ctx.clearRect(0, 0, width, height)

    // 构造网格:只画列分隔线,不铺满(铺满会变脏)
    ctx.save()
    ctx.strokeStyle = gridLine
    ctx.lineWidth = 1
    for (let c = 1; c < cols; c++) {
      const x = Math.round(boxX + colW * c) + 0.5
      ctx.beginPath()
      ctx.moveTo(x, boxY)
      ctx.lineTo(x, boxY + boxH)
      ctx.stroke()
    }
    ctx.restore()

    // 落点标记
    ctx.save()
    ctx.strokeStyle = gridLine
    for (let c = 0; c < cols; c++) {
      const x = colCenter(c)
      ctx.beginPath()
      ctx.moveTo(x - 3, boxY + boxH - 3.5)
      ctx.lineTo(x + 3, boxY + boxH - 3.5)
      ctx.stroke()
    }
    ctx.restore()

    // 块
    for (const { body, kind } of blocks) {
      const w = body.bounds.max.x - body.bounds.min.x
      const h = body.bounds.max.y - body.bounds.min.y
      const x = body.position.x - w / 2
      const y = body.position.y - h / 2
      ctx.save()
      if (kind === 'index') {
        ctx.fillStyle = ACCENT
        ctx.fillRect(Math.round(x) + 1, Math.round(y) + 1, Math.round(w) - 2, Math.max(3, Math.round(h) - 2))
      } else {
        ctx.fillStyle = blockFill
        ctx.fillRect(Math.round(x) + 1, Math.round(y) + 1, Math.round(w) - 2, Math.round(h) - 2)
        ctx.strokeStyle = blockLine
        ctx.lineWidth = 1
        ctx.strokeRect(Math.round(x) + 1.5, Math.round(y) + 1.5, Math.round(w) - 3, Math.round(h) - 3)
      }
      ctx.restore()
    }

    // 容器框:落定后渐入,2px 硬线 + 四角角标
    if (frameAlpha > 0) {
      ctx.save()
      ctx.globalAlpha = frameAlpha
      ctx.strokeStyle = blockLine
      ctx.lineWidth = 2
      ctx.strokeRect(boxX + 1, boxY + 1, boxW - 2, boxH - 2)
      const arm = 12
      ctx.lineWidth = 4
      ctx.beginPath()
      // 左上、右上
      ctx.moveTo(boxX, boxY + arm); ctx.lineTo(boxX, boxY); ctx.lineTo(boxX + arm, boxY)
      ctx.moveTo(boxX + boxW - arm, boxY); ctx.lineTo(boxX + boxW, boxY); ctx.lineTo(boxX + boxW, boxY + arm)
      // 左下、右下
      ctx.moveTo(boxX, boxY + boxH - arm); ctx.lineTo(boxX, boxY + boxH); ctx.lineTo(boxX + arm, boxY + boxH)
      ctx.moveTo(boxX + boxW - arm, boxY + boxH); ctx.lineTo(boxX + boxW, boxY + boxH); ctx.lineTo(boxX + boxW, boxY + boxH - arm)
      ctx.stroke()
      ctx.restore()
    }

    // 状态行:01 / DOCUMENTS → 02 / INDEXES → 03 / ONE .NODB
    const labels = ['01 / DOCUMENTS', '02 / INDEXES', '03 / ONE .NODB']
    ctx.save()
    ctx.font = `600 10px ${mono}`
    ctx.textBaseline = 'top'
    ctx.fillStyle = faint
    ctx.fillText(labels[stage], boxX, boxY - 20)
    if (frameAlpha > 0) {
      ctx.fillStyle = ACCENT
      ctx.fillRect(boxX + boxW - 5, boxY - 20, 5, 5)
    }
    ctx.restore()
  }

  /* ---------------- 静态终态(reduced-motion) ---------------- */

  if (reduceMotion) {
    for (const item of plan) spawn(item)
    for (let i = 0; i < 400; i++) Engine.update(engine, 1000 / 60)
    frameAlpha = 1
    stage = 2
    draw()
    cleanup.push(() => {
      Engine.clear(engine)
      canvas.remove()
    })
    return
  }

  /* ---------------- 循环 ---------------- */

  const runner = Runner.create()
  Runner.run(runner, engine)

  let raf = 0
  let last = performance.now()

  const loop = (now) => {
    const dt = Math.min(now - last, 64)
    last = now
    elapsed += dt

    while (plan.length && elapsed >= plan[0].at) spawn(plan.shift())

    if (stage === 0 && elapsed >= indexAt) stage = 1
    if (stage === 1 && elapsed >= frameAt) stage = 2
    if (stage === 2 && frameAlpha < 1) frameAlpha = Math.min(1, frameAlpha + dt / 240)

    draw()

    // 全部落定 + 框已成形 → 停止循环(之后画面是静态的,不必再烧 CPU)
    const settled = blocks.length > 0 && blocks.every((b) => b.body.isSleeping || b.body.speed < 0.12)
    if (settled && elapsed > frameAt + 900) {
      draw()
      return
    }
    raf = requestAnimationFrame(loop)
  }
  raf = requestAnimationFrame(loop)

  // 页面不可见时暂停
  const onVisibility = () => {
    if (document.hidden) {
      Runner.stop(runner)
      cancelAnimationFrame(raf)
    } else {
      Runner.run(runner, engine)
      last = performance.now()
      raf = requestAnimationFrame(loop)
    }
  }
  document.addEventListener('visibilitychange', onVisibility)

  // 窗口尺寸变化 → 重建(布局参数全部基于宽度)
  let resizeTimer = 0
  const onResize = () => {
    clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => {
      runCleanup()
      init()
    }, 400)
  }
  window.addEventListener('resize', onResize)

  cleanup.push(() => {
    clearTimeout(resizeTimer)
    cancelAnimationFrame(raf)
    Runner.stop(runner)
    Engine.clear(engine)
    document.removeEventListener('visibilitychange', onVisibility)
    window.removeEventListener('resize', onResize)
    canvas.remove()
  })
}

onMounted(init)

onBeforeUnmount(() => {
  runCleanup()
  lifecycle++
})

function runCleanup() {
  for (const fn of cleanup) {
    try {
      fn()
    } catch {
      /* 卸载阶段的清理失败不应影响其他清理 */
    }
  }
  cleanup = []
}
</script>

<template>
  <div ref="host" class="nb-hero-physics" />
</template>

<style scoped>
.nb-hero-physics {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 0;
}

@media (max-width: 640px) {
  .nb-hero-physics {
    opacity: 0.5;
  }
}
</style>
