import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = import.meta.dirname ?? fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(rootDir, 'src'),
      vue: resolve(rootDir, 'node_modules/vue'),
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
    outDir: resolve(rootDir, '../out/frontend/dist'),
    emptyOutDir: true,
    sourcemap: true,
  },
});
