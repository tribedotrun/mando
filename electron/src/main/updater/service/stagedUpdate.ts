import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'fs';
import path from 'path';
import { execFileSync, execSync } from 'child_process';
import { getAppBundlePath, getPendingPath, getStagingDir } from '#main/updater/config/updater';
import { pendingUpdateSchema, type PendingUpdate } from '#main/updater/types/updater';
import { errCode } from '#main/updater/service/updater';
import { parseJsonTextWith } from '#result';

export function downloadPath(): string {
  return path.join(getStagingDir(), 'update.zip');
}

export function ensureStagingDir(): void {
  mkdirSync(getStagingDir(), { recursive: true });
}

export function stagingAppExists(appPath: string): boolean {
  return existsSync(appPath);
}

export function extractAndStage(zipPath: string): string {
  const extractDir = path.join(getStagingDir(), 'extract');

  if (existsSync(extractDir)) rmSync(extractDir, { recursive: true });
  mkdirSync(extractDir, { recursive: true });

  execFileSync('ditto', ['-xk', zipPath, extractDir], { timeout: 120_000 });

  const entries = readdirSync(extractDir);
  const appEntry = entries.find((entry) => entry.endsWith('.app'));
  if (appEntry) return path.join(extractDir, appEntry);

  for (const entry of entries) {
    const nested = path.join(extractDir, entry);
    const nestedEntries = readdirSync(nested);
    const nestedApp = nestedEntries.find((child) => child.endsWith('.app'));
    if (nestedApp) return path.join(nested, nestedApp);
  }

  // invariant: extracted update ZIP must contain exactly one app bundle path.
  throw new Error('No .app bundle found in ZIP');
}

function verifyCodeSignature(appPath: string): void {
  execFileSync('codesign', ['--verify', '--deep', '--strict', appPath], {
    timeout: 30_000,
  });
}

export function applyStagedUpdate(newAppPath: string): void {
  const currentApp = getAppBundlePath();
  const oldAppPath = path.join(getStagingDir(), 'Mando-old.app');

  verifyCodeSignature(newAppPath);
  if (existsSync(oldAppPath)) rmSync(oldAppPath, { recursive: true });

  try {
    renameSync(currentApp, oldAppPath);
    try {
      renameSync(newAppPath, currentApp);
    } catch (innerErr) {
      renameSync(oldAppPath, currentApp);
      // invariant: a failed second rename must abort so the original app stays intact.
      throw innerErr;
    }
  } catch (err: unknown) {
    const code = errCode(err);
    if (code !== 'EPERM' && code !== 'EACCES') {
      // invariant: non-permission rename failures must abort the update swap.
      throw err;
    }
    if (!currentApp.endsWith('.app') || !currentApp.startsWith('/Applications/')) {
      // invariant: admin deletion is only safe for the expected /Applications bundle path.
      throw new Error(`auto-update: refusing admin rm on unexpected path: ${currentApp}`, {
        cause: err,
      });
    }
    execSync(
      `osascript -e 'do shell script "rm -rf \\"${currentApp}\\"" with administrator privileges'`,
      { timeout: 60_000 },
    );
    renameSync(newAppPath, currentApp);
  }
}

export function removePendingUpdateMarker(): void {
  rmSync(getPendingPath(), { force: true });
}

export function cleanupStagedUpdateArtifacts(): void {
  removePendingUpdateMarker();
  const oldAppPath = path.join(getStagingDir(), 'Mando-old.app');
  if (existsSync(oldAppPath)) {
    rmSync(oldAppPath, { recursive: true });
  }
  const extractDir = path.join(getStagingDir(), 'extract');
  if (existsSync(extractDir)) rmSync(extractDir, { recursive: true });
  const zipPath = downloadPath();
  if (existsSync(zipPath)) rmSync(zipPath);
}

export function writePendingUpdate(update: PendingUpdate): void {
  const pendingPath = getPendingPath();
  mkdirSync(path.dirname(pendingPath), { recursive: true });
  const tmpPath = `${pendingPath}.tmp`;
  writeFileSync(tmpPath, JSON.stringify(update), 'utf-8');
  renameSync(tmpPath, pendingPath);
}

export function readPendingUpdate(): PendingUpdate | null {
  try {
    const raw = readFileSync(getPendingPath(), 'utf-8');
    const parsed = parseJsonTextWith(raw, pendingUpdateSchema, 'file:pending-update');
    if (parsed.isErr()) {
      return null;
    }
    return parsed.value;
  } catch {
    return null;
  }
}
