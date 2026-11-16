import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

// 产物文件名固定,因为 Rust 单二进制用 include_bytes! 内嵌这些路径。
// 不固定(hash)会导致每次前端改动都要同步改 Rust 源码。
export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  // 相对 base:同一份产物既能被单二进制从根路径取用,
  // 也能原样放到 GitHub Pages 的 /nosqlite/playground/ 下。
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    rollupOptions: {
      output: {
        entryFileNames: 'assets/app.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name][extname]',
      },
    },
  },
});
