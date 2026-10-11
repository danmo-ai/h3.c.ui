import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { resolve } from 'path';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
      vue: resolve(__dirname, 'node_modules/vue'),
    },
    dedupe: ['vue'],
  },
  optimizeDeps: {
    include: ['reka-ui'],
    exclude: ['@danqing/dq-ui', '@danqing/dq-shell'],
  },
  server: {
    host: '127.0.0.1',
    port: 5810,
    strictPort: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:7810',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: resolve(__dirname, '../out/frontend/dist'),
    emptyOutDir: true,
    sourcemap: true,
  },
});
