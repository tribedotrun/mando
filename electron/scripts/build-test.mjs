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
 *   --dev  Inject VITE_DEV_SERVER_URL for HMR mode, but still build a static
 *          renderer fallback so daemon-led Electron relaunches can recover if
 *          the Vite server is gone.
 *          Set VITE_DEV_SERVER_URL env var to the running Vite dev server URL.
 */
import { build } from 'esbuild';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const buildDir = process.env.MANDO_BUILD_DIR || '.test-build';
const outDir = resolve(root, buildDir);

const isDev = process.argv.includes('--dev');
const viteUrl = process.env.VITE_DEV_SERVER_URL;

// Alias map: keep in sync with tsconfig.json `paths` and vite.renderer.config.mts.
const alias = {
  '#renderer': resolve(root, 'src/renderer'),
  '#main': resolve(root, 'src/main'),
  '#preload': resolve(root, 'src/preload'),
  '#shared': resolve(root, 'src/shared'),
  '#result': resolve(root, 'src/shared/result/index.ts'),
};

console.log('Building main process...');
await build({
  entryPoints: [resolve(root, 'src/main/index.ts')],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'cjs',
  external: ['electron'],
  outfile: resolve(outDir, 'main/index.js'),
  alias,
  define: {
    MAIN_WINDOW_VITE_DEV_SERVER_URL: isDev && viteUrl ? JSON.stringify(viteUrl) : 'undefined',
    MAIN_WINDOW_VITE_NAME: '"main_window"',
  },
  sourcemap: true,
});
console.log(`  -> ${buildDir}/main/index.js`);

console.log('Building preload script...');
await build({
  entryPoints: [resolve(root, 'src/preload/index.ts')],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'cjs',
  external: ['electron'],
  outfile: resolve(outDir, 'preload/index.js'),
  alias,
  sourcemap: true,
});
console.log(`  -> ${buildDir}/preload/index.js`);

console.log('Building renderer...');
const rendererRoot = resolve(root, 'src/renderer');
const rendererOut = resolve(outDir, 'renderer/main_window');
const rendererConfig = resolve(root, 'vite.renderer.config.mts');
execSync(
  `npx vite build "${rendererRoot}" -c "${rendererConfig}" --outDir "${rendererOut}" --emptyOutDir --minify false --logLevel warn`,
  {
    cwd: root,
    stdio: 'inherit',
  },
);
console.log(`  -> ${buildDir}/renderer/main_window/`);

if (isDev) {
  console.log(`\nDev mode — renderer served from Vite (${viteUrl || 'URL not set'}) with static fallback`);
}

console.log('\nTest build complete.');
