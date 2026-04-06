import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '#main': path.resolve(__dirname, 'src/main'),
      '#renderer': path.resolve(__dirname, 'src/renderer'),
      '#shared': path.resolve(__dirname, 'src/shared'),
    },
  },
  build: {
    outDir: '.vite/preload',
    rollupOptions: {
      external: ['electron'],
    },
  },
});
