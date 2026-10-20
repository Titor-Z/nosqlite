---
version: alpha
name: nodblite
description: 单文件 NoSQL 数据库的文档站设计系统 —— 深色优先、荧光绿强调、等宽字体的工程感

colors:
  # 深色(默认)
  bg: "#08090A"
  bg-elevated: "#0E1012"
  surface: "#131619"
  surface-hover: "#191D21"
  border: "#23282D"
  border-strong: "#333A41"
  text: "#E8EAED"
  text-muted: "#9AA0A6"
  text-faint: "#6B7178"

  # 强调色:荧光绿 —— 只在「数据在动」「可交互」处出现,不做装饰铺底
  accent: "#C6F24E"
  accent-hover: "#D4FF66"
  accent-dim: "#8FAE38"
  accent-glow: "rgba(198, 242, 78, 0.16)"

  # 语义色
  success: "#7BD88F"
  warning: "#E8B84B"
  danger: "#F2777A"
  info: "#7AA2F7"

  # 浅色
  bg-light: "#FAFAF8"
  surface-light: "#FFFFFF"
  border-light: "#E2E4E0"
  text-light: "#12141A"
  text-muted-light: "#5A6270"
  accent-light: "#3D5A00"

typography:
  display:
    fontFamily: "'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 64px
    fontWeight: 600
    lineHeight: 1.02
    letterSpacing: -0.03em
  h1:
    fontFamily: "'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 40px
    fontWeight: 600
    lineHeight: 1.12
    letterSpacing: -0.02em
  h2:
    fontFamily: "'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 28px
    fontWeight: 600
    lineHeight: 1.24
    letterSpacing: -0.015em
  h3:
    fontFamily: "'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.32
    letterSpacing: -0.01em
  body:
    fontFamily: "'Inter', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 16px
    fontWeight: 400
    lineHeight: 1.72
    letterSpacing: 0
  body-sm:
    fontFamily: "'Inter', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.65
  label:
    fontFamily: "'Inter', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 13px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.04em
  mono:
    fontFamily: "'JetBrains Mono', ui-monospace, monospace"
    fontSize: 13.5px
    fontWeight: 400
    lineHeight: 1.65
  stat:
    fontFamily: "'Space Grotesk', 'Noto Sans SC', system-ui, sans-serif"
    fontSize: 44px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: -0.03em
    fontFeature: "tnum"

rounded:
  sm: 4px
  md: 8px
  lg: 12px
  xl: 16px
  full: 999px

spacing:
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 40px
  2xl: 64px
  3xl: 96px

components:
  button-primary:
    background: "{colors.accent}"
    color: "{colors.bg}"
    rounded: "{rounded.md}"
    typography: "{typography.label}"
  button-ghost:
    background: transparent
    color: "{colors.text}"
    border: "{colors.border-strong}"
    rounded: "{rounded.md}"
  card:
    background: "{colors.surface}"
    border: "{colors.border}"
    rounded: "{rounded.lg}"
  code-block:
    background: "{colors.bg-elevated}"
    border: "{colors.border}"
    rounded: "{rounded.md}"
    typography: "{typography.mono}"
  nav:
    background: "{colors.bg}"
    border: "{colors.border}"
    typography: "{typography.label}"
---

# nodblite 设计系统

## Overview

**这个站要传达的气质:一件精密工具,不是一件消费品。**

nodblite 的定位是「部署形态学 SQLite,数据模型学 MongoDB」。它的用户是工程师 —— 会读基准数字、会在意 fsync 语义、会为了 30% 的写入差距改代码。所以站点不该像 SaaS 落地页那样热情洋溢,而应该像**一份写得好的技术报告**:密度高、数字准、不废话。

视觉上的三个决定:

1. **深色优先。** 数据库文档大多在编辑器旁边开着。深色不刺眼,也让荧光绿强调色有对比度可依。
2. **荧光绿(`accent`)是唯一的彩色。** 它只在两个地方出现:数据在动(hero 里的物理粒子)、可点击(主按钮、活跃导航项)。**不做装饰性铺底** —— 一旦强调色变成背景,它就失去了「看这里」的信号价值。
3. **等宽字体承担「真实」的重量。** 所有数字、代码、文件路径用 `JetBrains Mono`。这不是风格选择:性能报告里的 `7.98 ms` 必须看起来像是从终端里抄出来的,而不是排版出来的。

## Colors

### 深色基底

`bg` (#08090A) 近乎纯黑但带一点蓝,避免纯黑的「显示器关掉了」感。`surface` (#131619) 是卡片层,与 `bg` 的差异刻意做得小(约 2% 亮度)—— 靠 `border` 而不是靠明度差来分隔,这样层级多了也不会糊。

`border` (#23282D) 是分界线的主要手段。**不要用阴影表达层级** —— 深色背景上的阴影几乎看不见,只会让边缘变脏。

### 强调色

荧光绿 #C6F24E 在深色背景上对比度 **15.38:1**,远超 AAA。这是刻意的:它会被用在 13px 的小字上(导航、按钮标签),必须够亮。

浅色模式下换成 #3D5A00(深橄榄绿,7.55:1)—— 荧光绿在白底上完全不可读。

### 语义色

`success` / `warning` / `danger` / `info` 只用于表格里的标记和提示框,不与强调色争抢注意力。它们比常见的 Tailwind 语义色更柔和,因为这些颜色会出现在**性能表格**里 —— 数字旁边一排放射状的红色会让人误以为出了问题。

## Typography

**Space Grotesk**(标题)是一套几何无衬线,字形开口较大、字重饱满,在深色背景上比 Inter 更有存在感。它负责「这是一款现代工具」的第一印象。

**Inter**(正文)是当前屏幕阅读的事实标准,`1.72` 的行高在中文混排时也能保持呼吸感。

**JetBrains Mono**(代码与数字)的等宽数字(`tnum`)让性能表格的列天然对齐 —— 这省掉了手工调 `text-align` 的麻烦,也让数字更容易纵向比较。

中文字体统一回落到 **Noto Sans SC**。混排时英文用 Inter/Space Grotesk、中文用 Noto,视觉重量接近,不会出现「中文比英文重一档」的割裂。

## Layout

- 内容区最大宽度 **1280px**,正文段落限制在 **72ch**(约 720px)—— 超过这个宽度,眼睛回到下一行的起点会变累。
- 文档页左侧固定导航 240px,右侧目录 200px。移动端两侧折叠。
- 纵向节奏用 `spacing` 的 8px 网格:段落间 `md`(16px),章节间 `2xl`(64px),大区块间 `3xl`(96px)。
- **性能报告页例外**:表格允许超出正文宽度,用横向滚动而不是压缩列宽 —— 数字挤在一起就失去可读性。

## Elevation & Depth

深色界面里阴影基本无效,所以**层级靠边框和背景明度**:

| 层 | 背景 | 边框 |
|---|---|---|
| 页面 | `bg` | — |
| 卡片 | `surface` | `border` |
| 代码块 | `bg-elevated` | `border` |
| 悬浮态 | `surface-hover` | `border-strong` |

唯一使用发光(`accent-glow`)的地方是 hero 里的粒子与主按钮 hover —— 光代表「这里是活的」。

## Shapes

圆角统一偏小(`md` 8px 为主),因为这是工具而不是社交产品。代码块用 `md`,卡片用 `lg`(12px),按钮用 `md`。**不使用全圆角**,除了状态徽章。

## Components

### 按钮

主按钮是实心荧光绿 + 深色文字(对比度 15.4:1)。次按钮是透明底 + `border-strong` 描边。**没有第三种按钮** —— 页面上同时出现三种按钮权重,用户就不知道该点哪个。

### 代码块

背景 `bg-elevated`,带 1px `border`,不用阴影。行内代码用 `accent-dim` 前景 + 轻微背景色,确保在正文里能一眼跳出来但不抢戏。

### 表格

性能报告的核心载体。表头用 `label` 字号 + `text-muted`,数字列右对齐并启用 `tnum`。**数值越大越好的列用 `success` 标记,越慢越糟的用 `danger`** —— 但只在关键行,整表染色会让标记失去意义。

### 提示框

四类:`info`(蓝)、`tip`(绿)、`warning`(黄)、`danger`(红)。左边 2px 色条 + 轻微背景色,**不用整块填充** —— 文档里连续三个提示框会显得嘈杂。

## Do's and Don'ts

**Do**
- 用真实数字。`7.98 ms` 比「很快」有说服力。
- 让表格里的数字右对齐、等宽。
- 强调色只出现在「可点击」与「数据在动」两处。
- 中文与英文混排时检查视觉重量是否平衡。

**Don't**
- 不要用强调色做背景色块。
- 不要在深色背景上用阴影表达层级。
- 不要在正文里用超过 3 种字号。
- 不要把 Matter.js 用在按钮、下拉、导航这类高频交互上 —— 它们该用 CSS transition。
- 不要让 hero 的物理动画抢占 LCP:粒子层必须在文本渲染之后才启动。
