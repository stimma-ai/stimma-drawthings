import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// Built output is committed to manager-ui/dist and embedded in the Rust
// binary, served at /stp-v1/manage/. base './' keeps assets relative so the
// same build works behind the Stimma app's manager proxy.
export default defineConfig({
  plugins: [vue()],
  base: './',
  build: { sourcemap: false },
  server: {
    port: 5179,
    proxy: {
      '/stp-v1': { target: process.env.DRAWTHINGS || 'http://127.0.0.1:8765', changeOrigin: true },
    },
  },
})
