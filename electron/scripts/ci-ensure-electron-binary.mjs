/**
 * CI-only guard: guarantee node_modules/electron has a usable, launchable binary.
 *
 * Why this exists
 * ---------------
 * On the Linux CI runner, electron@41's postinstall (`node install.js`) downloads
 * the binary zip via `@electron/get` (this part works) and then unpacks it with
 * `extract-zip`. On the runner, that extraction STALLS on the first zip entry: the
 * extract promise never settles, nothing keeps the event loop alive, and Node exits
 * 0 WITHOUT writing `node_modules/electron/path.txt`. `npm ci` therefore reports
 * success while `node_modules/electron` has no binary, and Playwright's later
 * `electron.launch()` dies with "Electron failed to install correctly".
 *
 * Reproduced deterministically in node:20 and node:24 on linux/amd64. The download
 * is fine; only electron's bundled `extract-zip` is broken in this environment.
 *
 * What this does
 * --------------
 * If electron already resolves to an existing binary, do nothing. Otherwise download
 * the artifact with electron's own `@electron/get` (so version/mirror/checksum logic
 * matches upstream exactly), extract the cached zip with the system `unzip` (reliable
 * where extract-zip stalls), write `path.txt` ourselves, and hard-assert the result is
 * a present, executable, resolvable binary. Any failure — including a real download or
 * rate-limit error — exits non-zero LOUDLY instead of leaving a silent gap for
 * Playwright to trip over.
 *
 * Invoked from .github/workflows/ci-checks.yml (electron-integration job) with the
 * working directory set to `electron/`. `npm ci` runs with ELECTRON_SKIP_BINARY_DOWNLOAD=1
 * there, so this script is the single deterministic source of truth for the binary.
 */
import { execFileSync } from 'node:child_process';
import {
  accessSync,
  chmodSync,
  constants as fsConstants,
  existsSync,
  mkdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

const electronPkgDir = path.resolve(process.cwd(), 'node_modules', 'electron');
if (!existsSync(electronPkgDir)) {
  console.error(`[ensure-electron] ${electronPkgDir} not found — run npm ci first.`);
  process.exit(1);
}

// Resolve modules and data files from electron's own perspective so nested vs
// hoisted layouts and electron's pinned @electron/get version both work.
const requireFromElectron = createRequire(path.join(electronPkgDir, 'index.js'));
const { version } = requireFromElectron('./package.json');

// Matches getPlatformPath() in electron/install.js.
function platformExecutablePath() {
  switch (process.platform) {
    case 'win32':
      return 'electron.exe';
    case 'mas':
    case 'darwin':
      return 'Electron.app/Contents/MacOS/Electron';
    default:
      return 'electron';
  }
}

const platformPath = platformExecutablePath();
const distDir = path.join(electronPkgDir, 'dist');
const binaryPath = path.join(distDir, platformPath);
const pathTxt = path.join(electronPkgDir, 'path.txt');

// Fast path: a healthy install already present (extract-zip worked, or macOS dev).
if (existsSync(binaryPath) && existsSync(pathTxt)) {
  console.log(`[ensure-electron] already installed: ${binaryPath}`);
  process.exit(0);
}

console.log(`[ensure-electron] electron@${version} missing on ${process.platform}/${process.arch}; installing.`);

const zipPath = await downloadArtifactZip();
extractWithUnzip(zipPath);
writeFileSync(pathTxt, platformPath);
assertLaunchable();

console.log('[ensure-electron] done.');

async function downloadArtifactZip() {
  const { downloadArtifact } = requireFromElectron('@electron/get');
  // Mirror install.js: verify against the bundled checksums.json.
  const checksums = requireFromElectron('./checksums.json');
  const zip = await downloadArtifact({
    version,
    artifactName: 'electron',
    platform: process.platform,
    arch: process.arch,
    checksums,
  });
  console.log(`[ensure-electron] downloaded artifact: ${zip}`);
  return zip;
}

function extractWithUnzip(zip) {
  rmSync(distDir, { recursive: true, force: true });
  mkdirSync(distDir, { recursive: true });
  // `unzip` restores unix permissions and symlinks from the zip's external
  // attributes; electron's Linux artifact relies on both.
  execFileSync('unzip', ['-oq', zip, '-d', distDir], { stdio: 'inherit' });
  console.log(`[ensure-electron] extracted into ${distDir}`);
}

function assertLaunchable() {
  if (!existsSync(binaryPath)) {
    throw new Error(`[ensure-electron] binary missing after extract: ${binaryPath}`);
  }
  if (process.platform !== 'win32') {
    // Defensive: ensure the launcher bit survived extraction.
    chmodSync(binaryPath, 0o755);
    accessSync(binaryPath, fsConstants.X_OK);
  }
  // index.js' getElectronPath() throws unless path.txt + dist are consistent.
  const resolved = requireFromElectron('./index.js');
  if (resolved !== binaryPath) {
    throw new Error(`[ensure-electron] electron resolved to ${resolved}, expected ${binaryPath}`);
  }
  console.log(`[ensure-electron] verified launchable binary: ${resolved}`);
}
