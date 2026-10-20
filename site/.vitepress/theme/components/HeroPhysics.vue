<script setup>
/**
 * 首页 hero 背景:文档粒子落进一个 .nodb 容器。
 *
 * 为什么用 Matter.js 而不是 CSS:
 * 这里的动效是「解释」—— 让「一堆文档 → 一个文件」这件事在 2 秒内被看懂。
 * 粒子之间的碰撞、堆叠、沉降没有 CSS 等价物。这是营销页的装饰性动效,
 * 属于可以承担 ~90KB 的场景。
 *
 * 明确不做的事:
 *   - 不在按钮/下拉/导航上用物理引擎(它们该用 CSS transition);
 *   - 不阻塞 LCP:等 window load + 一帧之后才启动仿真;
 *   - 尊重 prefers-reduced-motion:直接画静态终态,不跑循环;
 *   - 页面不可见时暂停(visibilitychange),避免后台烧 CPU。
 */
import { onMounted, onBeforeUnmount, ref } from 'vue'

const host = ref(null)

let engine = null
let runner = null
let render = null
let cleanup = []

onMounted(async () => {
  const el = host.value
  if (!el) return

  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

  // 等首屏文本渲染完再启动,避免抢占 LCP
  await new Promise((r) => (document.readyState === 'complete' ? r() : window.addEventListener('load', r, { once: true })))
  await new Promise((r) => requestAnimationFrame(r))

  const Matter = (await import('matter-js')).default
  const { Engine, Runner, Bodies, Composite, Mouse, MouseConstraint, Events } = Matter

  // 关键:画布必须挂在 .VPHero 里,而不是本组件的容器。
  // 本组件渲染在 markdown 内容槽里,外面包着 VitePress 的
  // position:relative 包装 div(在 hero 之后),CSS 无法跨过它定位。
  // 直接把宿主容器 DOM 移动到 hero 内,后续的尺寸/定位都以 hero 为准。
  const hero = document.querySelector('.VPHero')
  if (hero && el.parentElement !== hero) {
    hero.appendChild(el)
  }

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

  const styles = getComputedStyle(document.documentElement)
  const accent = styles.getPropertyValue('--nb-accent').trim() || '#C6F24E'
  const border = styles.getPropertyValue('--nb-border').trim() || '#23282D'
  const textFaint = styles.getPropertyValue('--nb-text-faint').trim() || '#6B7178'

  engine = Engine.create({ gravity: { x: 0, y: 0.55 } })

  // 容器:一个开口朝上的盒子,粒子从上方落进去。
  // 桌面在 hero 右侧(默认 logo 的位置);移动端缩小放到底部,
  // 且与 hero 文本/按钮错开(按钮约占高度 55%)。
  // 墙体故意加厚并往外侧偏移:薄墙 + 高速粒子会隧穿。
  const isMobile = width < 640
  const boxW = isMobile ? Math.min(width * 0.55, 220) : Math.min(width * 0.28, 340)
  const boxH = isMobile ? 110 : 260
  const boxX = isMobile ? (width - boxW) / 2 : width - boxW - Math.max(width * 0.06, 40)
  const boxY = isMobile ? height - boxH - 8 : height - boxH - 30
  const wall = 60

  const walls = [
    Bodies.rectangle(boxX - wall / 2, boxY + boxH / 2, wall, boxH * 2, { isStatic: true }),
    Bodies.rectangle(boxX + boxW + wall / 2, boxY + boxH / 2, wall, boxH * 2, { isStatic: true }),
    Bodies.rectangle(boxX + boxW / 2, boxY + boxH + wall / 2, boxW + wall * 2, wall, { isStatic: true }),
  ]

  // 文档粒子:大小不一,模拟不同长度的文档。
  // 出生点限制在盒口上方一小段 —— 太高的落差会让落地速度超过求解器的
  // 每步位移上限,同样会隧穿。移动端粒子更少更小。
  const particles = []
  const count = isMobile ? 18 : 46
  for (let i = 0; i < count; i++) {
    const w = isMobile ? 8 + Math.random() * 16 : 10 + Math.random() * 26
    const h = 5 + Math.random() * 3
    particles.push(
      Bodies.rectangle(
        boxX + boxW * (0.15 + Math.random() * 0.7),
        boxY - 16 - Math.random() * (isMobile ? 120 : 260),
        w,
        h,
        {
          restitution: 0.28,
          friction: 0.35,
          frictionAir: 0.012,
          angle: Math.random() * Math.PI,
          chamfer: { radius: 1.5 },
        },
      ),
    )
  }

  Composite.add(engine.world, [...walls, ...particles])

  // 绘制:每帧读取 body 状态,画成细长的小方块(文档意象)
  const draw = () => {
    ctx.clearRect(0, 0, width, height)

    // 容器轮廓:虚线,表示「文件边界」
    ctx.save()
    ctx.strokeStyle = border
    ctx.lineWidth = 1
    ctx.setLineDash([4, 4])
    ctx.beginPath()
    ctx.moveTo(boxX, boxY)
    ctx.lineTo(boxX, boxY + boxH)
    ctx.lineTo(boxX + boxW, boxY + boxH)
    ctx.lineTo(boxX + boxW, boxY)
    ctx.stroke()
    ctx.restore()

    // 粒子
    for (const b of particles) {
      const w = b.bounds.max.x - b.bounds.min.x
      ctx.save()
      ctx.translate(b.position.x, b.position.y)
      ctx.rotate(b.angle)
      ctx.fillStyle = accent
      ctx.globalAlpha = 0.75
      ctx.fillRect(-w / 2, -3, w, 6)
      ctx.restore()
    }

    // 容器底部标注
    ctx.save()
    ctx.fillStyle = textFaint
    ctx.font = `500 11px ${styles.getPropertyValue('--vp-font-family-mono') || 'monospace'}`
    ctx.textAlign = 'center'
    ctx.fillText('one .nodb file', boxX + boxW / 2, boxY + boxH + 26)
    ctx.restore()
  }

  if (reduceMotion) {
    // 静态终态:直接跑若干步让粒子落定,画一次,不启动循环
    for (let i = 0; i < 240; i++) Engine.update(engine, 1000 / 60)
    draw()
    cleanup.push(() => canvas.remove())
    return
  }

  // 鼠标交互:可以拨动粒子。只在精确指针设备上启用。
  if (window.matchMedia('(hover: hover) and (pointer: fine)').matches) {
    const mouse = Mouse.create(canvas)
    const mc = MouseConstraint.create(engine, {
      mouse,
      constraint: { stiffness: 0.12, render: { visible: false } },
    })
    Composite.add(engine.world, mc)
    // Mouse.create 会绑 wheel 事件,这里解绑避免滚动页面被吃掉
    mouse.element.removeEventListener('wheel', mouse.mousewheel)
    cleanup.push(() => Composite.remove(engine.world, mc))
  }

  runner = Runner.create()
  Runner.run(runner, engine)

  let raf = 0
  const loop = () => {
    draw()
    raf = requestAnimationFrame(loop)
  }
  loop()

  // 页面不可见时暂停,避免后台烧 CPU
  const onVisibility = () => {
    if (document.hidden) {
      Runner.stop(runner)
      cancelAnimationFrame(raf)
    } else {
      Runner.run(runner, engine)
      loop()
    }
  }
  document.addEventListener('visibilitychange', onVisibility)
  cleanup.push(() => document.removeEventListener('visibilitychange', onVisibility))

  cleanup.push(() => {
    cancelAnimationFrame(raf)
    Runner.stop(runner)
    Engine.clear(engine)
    canvas.remove()
  })
})

onBeforeUnmount(() => {
  for (const fn of cleanup) {
    try {
      fn()
    } catch {
      /* 卸载阶段的清理失败不应影响其他清理 */
    }
  }
  cleanup = []
})
</script>

<template>
  <div ref="host" class="nb-hero-physics" />
</template>

<style scoped>
.nb-hero-physics {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: auto;
  /* 在 hero 文本之下,不抢文字可读性 */
  z-index: 0;
}

/* 移动端与减少动效时不占位,避免空白 */
@media (max-width: 640px) {
  .nb-hero-physics {
    opacity: 0.5;
  }
}
</style>
