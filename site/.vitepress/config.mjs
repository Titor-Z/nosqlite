import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'nodblite',
  description:
    '像 SQLite 一样小巧的单文件 NoSQL 数据库 —— MongoDB 风格文档模型,单文件零配置部署',

  // GitHub Pages 项目站点,路径前缀必须与仓库名一致
  base: '/nosqlite/',
  cleanUrls: true,
  lastUpdated: true,
  ignoreDeadLinks: false,

  // 构成主义以纸感为主:纸底长文更耐读,暗色主题保留为可选项。
  appearance: 'light',

  head: [
    ['link', { rel: 'icon', href: '/nosqlite/favicon.svg', type: 'image/svg+xml' }],
    ['meta', { name: 'theme-color', content: '#F2EDE3' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'nodblite' }],
    [
      'meta',
      {
        property: 'og:description',
        content: '单文件 NoSQL 数据库。MongoDB 语义,SQLite 部署形态。',
      },
    ],
  ],

  themeConfig: {
    logo: { light: '/logo.svg', dark: '/logo-dark.svg' },

    nav: [
      { text: '指南', link: '/quickstart/rust', activeMatch: '/quickstart/' },
      { text: '使用', link: '/guide/query', activeMatch: '/guide/' },
      { text: '性能', link: '/reference/performance' },
      { text: '兼容性', link: '/reference/compatibility' },
      { text: 'FAQ', link: '/reference/faq' },
      // 静态目录不是 VitePress 路由,必须用绝对地址整页跳转
      { text: 'Playground', link: 'https://titor-z.github.io/nosqlite/playground/' },
      {
        text: 'v0.6.11',
        items: [
          { text: '更新日志', link: 'https://github.com/Titor-Z/nosqlite/blob/main/CHANGELOG.md' },
          { text: 'GitHub Releases', link: 'https://github.com/Titor-Z/nosqlite/releases' },
        ],
      },
    ],

    sidebar: {
      '/quickstart/': [
        {
          text: '快速开始',
          items: [
            { text: 'Rust 内嵌', link: '/quickstart/rust' },
            { text: 'C ABI', link: '/quickstart/c-api' },
            { text: 'JavaScript', link: '/quickstart/javascript' },
            { text: 'wasm(浏览器 / 边缘)', link: '/quickstart/wasm' },
            { text: '命令行', link: '/quickstart/cli' },
            { text: 'MongoDB 驱动', link: '/quickstart/wire' },
          ],
        },
      ],
      '/guide/': [
        {
          text: '使用指南',
          items: [
            { text: '查询', link: '/guide/query' },
            { text: '更新', link: '/guide/update' },
            { text: '投影', link: '/guide/projection' },
            { text: '索引', link: '/guide/indexes' },
            { text: '聚合', link: '/guide/aggregate' },
            { text: '账号与认证', link: '/guide/auth' },
          ],
        },
      ],
      '/reference/': [
        {
          text: '参考',
          items: [
            { text: '性能报告', link: '/reference/performance' },
            { text: '与 MongoDB 的差异', link: '/reference/compatibility' },
            { text: '常见问题', link: '/reference/faq' },
          ],
        },
      ],
    },

    socialLinks: [{ icon: 'github', link: 'https://github.com/Titor-Z/nosqlite' }],

    editLink: {
      pattern: 'https://github.com/Titor-Z/nosqlite/edit/main/docs/:path',
      text: '在 GitHub 上编辑此页',
    },

    search: {
      provider: 'local',
      options: {
        translations: {
          button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
          modal: {
            noResultsText: '没有找到结果',
            resetButtonTitle: '清除条件',
            footer: {
              selectText: '选择',
              navigateText: '切换',
              closeText: '关闭',
            },
          },
        },
      },
    },

    outline: { level: [2, 3], label: '本页目录' },

    docFooter: { prev: '上一篇', next: '下一篇' },

    lastUpdated: { text: '最后更新于' },

    footer: {
      message: 'Apache-2.0 许可 · 单文件、零配置、进程内',
      copyright: 'nodblite',
    },
  },

  markdown: {
    // 低饱和高亮,与纸/墨/朱红同族(默认 github-light 的紫蓝与站点头色冲突)
    theme: { light: 'min-light', dark: 'min-dark' },
    lineNumbers: false,
  },
})
