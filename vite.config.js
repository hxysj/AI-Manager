import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

const devPort = Number.parseInt(process.env.AI_MANAGER_DEV_PORT || '13134', 10)

export default defineConfig({
  base: './',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  server: {
    host: '127.0.0.1',
    port: Number.isFinite(devPort) ? devPort : 13134,
    // 直接启动渲染器时自动顺延；Tauri 启动器会先选好端口并传入环境变量。
    strictPort: Boolean(process.env.AI_MANAGER_DEV_PORT)
  },
  build: {
    chunkSizeWarningLimit: 2500
  }
})
