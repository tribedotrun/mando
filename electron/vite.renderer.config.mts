import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { defineConfig, Plugin } from 'vite';
import tailwindcss from '@tailwindcss/vite';

const pkg = JSON.parse(readFileSync(new URL('package.json', import.meta.url), 'utf-8'));

/** Strip Vite-injected CSP meta tags — the daemon controls CSP via response headers. */
function stripCspMeta(): Plugin {
  return {
    name: 'strip-csp-meta',
    enforce: 'post',
    transformIndexHtml(html) {
      return html.replace(/<meta\s+http-equiv="Content-Security-Policy"[^>]*\/?\s*>\s*/g, '');
    },
  };
}

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  plugins: [tailwindcss(), stripCspMeta()],
  resolve: {
    alias: {
      '#renderer': fileURLToPath(new URL('src/renderer', import.meta.url)),
      '#main': fileURLToPath(new URL('src/main', import.meta.url)),
      '#shared': fileURLToPath(new URL('src/shared', import.meta.url)),
      '#contracts': fileURLToPath(new URL('../contracts', import.meta.url)),
    },
  },
  base: './',
  server: {
    host: '127.0.0.1',
    port: 15173,
    strictPort: true,
  },
  build: {
    rollupOptions: {
      onwarn(warning, warn) {
        if (warning.code === 'MODULE_LEVEL_DIRECTIVE' && warning.message.includes('"use client"')) {
          return;
        }
        if (warning.code === 'SOURCEMAP_ERROR' && warning.id?.includes('/node_modules/')) {
          return;
        }
        warn(warning);
      },
      output: {
        manualChunks(id) {
          if (id.includes('/node_modules/react-dom/') || id.includes('/node_modules/react/')) {
            return 'vendor-react';
          }
          if (id.includes('/node_modules/@tanstack/react-query/')) {
            return 'vendor-query';
          }
          if (id.includes('/node_modules/react-markdown/')) {
            return 'vendor-markdown';
          }
          if (id.includes('/node_modules/radix-ui/') || id.includes('/node_modules/@radix-ui/')) {
            return 'vendor-radix';
          }
        },
      },
    },
  },
});
