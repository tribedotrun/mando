// Behavior-level test for the sidebar Update click path. Complements the
// static-source scan in `installHandlerShape.test.ts` by exercising the
// real function with mocked dependencies and asserting the exact call
// graph and ordering — not just absence of forbidden tokens in source.
//
// The bug this guards against: before the fix, clicking Update ran daemon
// bootout + .app swap in-process, so the renderer flashed
// "Daemon disconnected — Reconnecting…" while SSE was down. The fix
// requires the handler to announce `Updating` to the daemon first, then
// relaunch + exit Electron — so the daemon stays alive while Electron
// quits and `applyPendingUpdateIfAny()` on the next boot does the swap
// with no UI mounted.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { quitForPendingUpdate } from '../runtime/installClickPath.ts';

function recorderDeps() {
  const calls: string[] = [];
  return {
    calls,
    announceUiUpdating: async () => {
      calls.push('announce');
      return true;
    },
    relaunch: () => {
      calls.push('relaunch');
    },
    exit: (code: number) => {
      calls.push(`exit:${code}`);
    },
  };
}

describe('quitForPendingUpdate (sidebar Update click path)', () => {
  it('calls announceUiUpdating, then relaunch, then exit(0) — in that exact order', async () => {
    const deps = recorderDeps();
    await quitForPendingUpdate(deps);
    assert.deepEqual(deps.calls, ['announce', 'relaunch', 'exit:0']);
  });

  it('awaits announceUiUpdating before relaunching (renderer must announce Updating before quitting)', async () => {
    const calls: string[] = [];
    let resolveAnnounce: (() => void) | null = null;
    const announcePromise = new Promise<void>((r) => {
      resolveAnnounce = r;
    });

    const deps = {
      announceUiUpdating: async () => {
        calls.push('announce:start');
        await announcePromise;
        calls.push('announce:done');
        return true;
      },
      relaunch: () => {
        calls.push('relaunch');
      },
      exit: () => {
        calls.push('exit');
      },
    };

    const inflight = quitForPendingUpdate(deps);
    // Give the microtask queue a tick so announce can start.
    await Promise.resolve();
    assert.deepEqual(
      calls,
      ['announce:start'],
      'relaunch must not fire before announceUiUpdating resolves',
    );

    resolveAnnounce!();
    await inflight;
    assert.deepEqual(calls, ['announce:start', 'announce:done', 'relaunch', 'exit']);
  });

  it('still relaunches and exits if announceUiUpdating returns false (daemon supervisor will catch up)', async () => {
    const calls: string[] = [];
    await quitForPendingUpdate({
      announceUiUpdating: async () => {
        calls.push('announce');
        return false;
      },
      relaunch: () => calls.push('relaunch'),
      exit: () => calls.push('exit'),
    });
    assert.deepEqual(calls, ['announce', 'relaunch', 'exit']);
  });
});
