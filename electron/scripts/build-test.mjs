/**
 * Build script for E2E testing — compiles main, preload, and renderer
 * without needing Electron Forge's dev server.
 *
 * Output layout (.test-build/):
 *   main/index.js       — main process (esbuild)
 *   preload/index.js     — preload script (esbuild)
 *   renderer/main_window/ — renderer app (vite build)
 *
 * The directory structure matches what index.ts expects:
 *   preload:  path.join(__dirname, '../preload/index.js')
 *   renderer: path.join(__dirname, '../renderer/${MAIN_WINDOW_VITE_NAME}/index.html')
 *
 * Flags:
 *   --dev  Skip renderer build; inject VITE_DEV_SERVER_URL for HMR mode.
 *          Set VITE_DEV_SERVER_URL env var to the running Vite dev server URL.
 */
import { build } from 'esbuild';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const outDir = resolve(root, '.test-build');

const isDev = process.argv.includes('--dev');
const viteUrl = process.env.VITE_DEV_SERVER_URL;

console.log('Building main process...');
await build({
  entryPoints: [resolve(root, 'src/main/index.ts')],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'cjs',
  external: ['electron'],
  outfile: resolve(outDir, 'main/index.js'),
  define: {
    MAIN_WINDOW_VITE_DEV_SERVER_URL: isDev && viteUrl ? JSON.stringify(viteUrl) : 'undefined',
    MAIN_WINDOW_VITE_NAME: '"main_window"',
  },
  sourcemap: true,
});
console.log('  -> .test-build/main/index.js');

console.log('Building preload script...');
await build({
  entryPoints: [resolve(root, 'src/preload/index.ts')],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'cjs',
  external: ['electron'],
  outfile: resolve(outDir, 'preload/index.js'),
  sourcemap: true,
});
console.log('  -> .test-build/preload/index.js');

if (isDev) {
  console.log(`\nDev mode — renderer served from Vite (${viteUrl || 'URL not set'})`);
} else {
  console.log('Building renderer...');
  const rendererRoot = resolve(root, 'src/renderer');
  const rendererOut = resolve(outDir, 'renderer/main_window');
  const rendererConfig = resolve(root, 'vite.renderer.config.mts');
  execSync(
    `npx vite build "${rendererRoot}" -c "${rendererConfig}" --outDir "${rendererOut}" --emptyOutDir --minify false`,
    {
      cwd: root,
      stdio: 'inherit',
    },
  );
  console.log('  -> .test-build/renderer/main_window/');
}

console.log('\nTest build complete.');
