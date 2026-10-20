// nodblite 站点主题入口。
//
// 只做三件事:
//   1. 继承 VitePress 默认主题(导航/侧栏/搜索/大纲全都复用,不重造);
//   2. 覆盖配色与排版(见 style.css,设计依据见仓库根目录 DESIGN.md);
//   3. 注册首页的物理动效组件。

import DefaultTheme from 'vitepress/theme'
import './style.css'

import HeroPhysics from './components/HeroPhysics.vue'
import PerfTable from './components/PerfTable.vue'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('HeroPhysics', HeroPhysics)
    app.component('PerfTable', PerfTable)
  },
}
