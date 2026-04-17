import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '#main': path.resolve(__dirname, 'src/main'),
      '#renderer': path.resolve(__dirname, 'src/renderer'),
      '#preload': path.resolve(__dirname, 'src/preload'),
      '#shared': path.resolve(__dirname, 'src/shared'),
    },
  },
  build: {
    rollupOptions: {
      external: ['electron', /\.node$/],
    },
  },
});
