// Regression: clicking sidebar Update used to freeze the renderer for several
// seconds because the IPC `updates:install` handler ran a chain of `execSync`
// / `execFileSync` calls (launchctl bootout, busy-loop sleep polling, codesign
// --verify) on the Electron main process event loop. Renderer↔main IPC and
// every BrowserWindow frame stalled until the chain finished.
//
// The fix converts every shell call in the install path to async
// (`promisify(execFile)` + `await new Promise(setTimeout)`). This test pins
// that invariant: if anyone reintroduces a sync exec call into the install
// chain, it fails loudly.
//
// The check identifies each sync exec call by its enclosing function name
// and asserts the set matches a documented allowlist exactly. Comparing call
// sites — not just counts — catches the "remove one allowed call and add a
// forbidden one in the same file" case.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';

type SyncCallName = 'execSync' | 'execFileSync' | 'spawnSync';

interface SyncCallSite {
  enclosingFunction: string;
  call: SyncCallName;
}

interface AllowedSyncCall extends SyncCallSite {
  reason: string;
}

interface InstallPathFile {
  relPath: string;
  allowedSyncCalls: ReadonlyArray<AllowedSyncCall>;
}

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..', '..', '..', '..');

const INSTALL_PATH_FILES: ReadonlyArray<InstallPathFile> = [
  {
    relPath: 'electron/src/main/global/runtime/portCheck.ts',
    allowedSyncCalls: [
      {
        enclosingFunction: 'getDaemonStatus',
        call: 'execSync',
        reason:
          'dead-code reexport with no install-path caller; not on the sidebar Update click path',
      },
    ],
  },
  {
    relPath: 'electron/src/main/global/runtime/launchdServices.ts',
    allowedSyncCalls: [],
  },
  {
    relPath: 'electron/src/main/global/runtime/launchdInstall.ts',
    allowedSyncCalls: [],
  },
  {
    relPath: 'electron/src/main/updater/service/stagedUpdate.ts',
    allowedSyncCalls: [
      {
        enclosingFunction: 'extractAndStage',
        call: 'execFileSync',
        reason: 'runs at download time (background), before the user clicks Update',
      },
    ],
  },
  {
    relPath: 'electron/src/main/updater/runtime/applyPendingUpdateFlow.ts',
    allowedSyncCalls: [],
  },
];

const SYNC_CALL_PATTERN = /\b(execSync|execFileSync|spawnSync)\s*\(/;
const FUNCTION_DECL_PATTERN = /(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)/;

async function findSyncExecSites(absPath: string): Promise<SyncCallSite[]> {
  const text = await readFile(absPath, 'utf-8');
  const lines = text.split('\n');
  const sites: SyncCallSite[] = [];
  let enclosingFunction = '<top-level>';
  for (const line of lines) {
    const fnMatch = line.match(FUNCTION_DECL_PATTERN);
    if (fnMatch) {
      enclosingFunction = fnMatch[1]!;
    }
    const syncMatch = line.match(SYNC_CALL_PATTERN);
    if (syncMatch) {
      sites.push({ enclosingFunction, call: syncMatch[1] as SyncCallName });
    }
  }
  return sites;
}

function siteKey(s: SyncCallSite): string {
  return `${s.enclosingFunction}::${s.call}`;
}

describe('install path is non-blocking on the Electron main event loop', () => {
  for (const entry of INSTALL_PATH_FILES) {
    it(`${entry.relPath} sync-exec call sites match the documented allowlist exactly`, async () => {
      const actual = await findSyncExecSites(path.join(REPO_ROOT, entry.relPath));
      const actualKeys = actual.map(siteKey).sort();
      const expectedKeys = entry.allowedSyncCalls.map(siteKey).sort();

      assert.deepEqual(
        actualKeys,
        expectedKeys,
        `Sync exec call sites in ${entry.relPath} drifted from the allowlist.\n` +
          `  actual:   ${JSON.stringify(actualKeys)}\n` +
          `  expected: ${JSON.stringify(expectedKeys)}\n` +
          `If a new sync call is legitimate (e.g., a new dead-code helper not on the install click path),\n` +
          `add it to allowedSyncCalls with a short reason. Otherwise convert it to async.`,
      );
    });
  }
});
