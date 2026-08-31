import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '#preload': path.resolve(__dirname, 'src/preload'),
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
