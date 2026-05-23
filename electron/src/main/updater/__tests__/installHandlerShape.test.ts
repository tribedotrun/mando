// Regression: the sidebar Update click handler used to call
// `applyPendingUpdateFlow(pendingUpdate, ...)` directly, which ran daemon
// bootout → binary swap → bootstrap in-process while the renderer was still
// alive. The renderer's SSE connection dropped during the bootout window
// and `AppLayout.tsx` flashed a "Daemon disconnected — Reconnecting…"
// banner for several seconds before the window finally closed.
//
// The fix defers the heavy work to the startup-time `applyPendingUpdateIfAny()`
// path. The click handler now only announces `Updating` to the daemon, then
// calls `app.relaunch()` + `app.exit(0)`. The daemon stays up while Electron
// quits; the next process boot performs the bootout+swap before any
// BrowserWindow is created.
//
// This test pins the invariant by scanning the `handleChannel('updates:install', ...)`
// block in `runtime/updater.ts` and asserting it does NOT reference the
// heavy in-process calls. If anyone re-introduces them inside the handler,
// it fails loudly — and it would have failed on the pre-fix code.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..', '..', '..', '..');
const UPDATER_PATH = path.join(REPO_ROOT, 'electron/src/main/updater/runtime/updater.ts');

const FORBIDDEN_CALLS_IN_INSTALL_HANDLER = [
  {
    pattern: /applyPendingUpdateFlow\s*\(/,
    name: 'applyPendingUpdateFlow(',
    reason:
      'runs daemon bootout + .app swap synchronously in-process; tears down SSE while the renderer is still alive',
  },
  {
    pattern: /updateDaemonBinary\s*\(/,
    name: 'updateDaemonBinary(',
    reason: 'calls launchctl bootout while the renderer is alive — flashes the disconnect banner',
  },
  {
    pattern: /applyStagedUpdate\s*\(/,
    name: 'applyStagedUpdate(',
    reason:
      'swaps the .app bundle in-process; pair work with the daemon teardown belongs to the startup path',
  },
];

async function readInstallHandlerBlock(): Promise<string> {
  const src = await readFile(UPDATER_PATH, 'utf-8');
  const start = src.indexOf("handleChannel('updates:install',");
  assert.ok(
    start >= 0,
    `expected handleChannel('updates:install', ...) in ${UPDATER_PATH}; if you renamed the channel, update this test.`,
  );
  // The install handler is followed by `handleChannel('updates:check', ...)`.
  // We slice between the two so the scan is scoped to the install handler body
  // and doesn't leak into adjacent handlers or top-level helpers.
  const tail = src.slice(start);
  const next = tail.indexOf("handleChannel('updates:check'");
  assert.ok(
    next > 0,
    `expected handleChannel('updates:check', ...) to follow the install handler in ${UPDATER_PATH}; if the order changed, update this test.`,
  );
  return tail.slice(0, next);
}

describe('updates:install handler shape (regression: avoid daemon teardown while UI alive)', () => {
  for (const forbidden of FORBIDDEN_CALLS_IN_INSTALL_HANDLER) {
    it(`does not invoke ${forbidden.name} inside the install handler`, async () => {
      const block = await readInstallHandlerBlock();
      assert.ok(
        !forbidden.pattern.test(block),
        `${forbidden.name} appeared inside the updates:install handler.\n` +
          `Reason this is banned: ${forbidden.reason}.\n` +
          `Defer the work to applyPendingUpdateIfAny() on the next process boot instead.`,
      );
    });
  }

  it('calls app.relaunch() and app.exit() inside the install handler', async () => {
    const block = await readInstallHandlerBlock();
    assert.ok(
      /app\.relaunch\s*\(/.test(block),
      'install handler must call app.relaunch() to bring up the post-quit process that runs applyPendingUpdateIfAny()',
    );
    assert.ok(
      /app\.exit\s*\(/.test(block),
      'install handler must call app.exit() so before-quit fires and Electron tears down before the daemon does',
    );
  });
});
