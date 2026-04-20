/**
 * Owner for the quit flow. Single authority for "is the app quitting"
 * and "has the daemon been notified of our quit". Replaces the ambient
 * `isQuitting`/`quitAnnounced` flags that used to live at module scope
 * in `main/index.ts`.
 *
 * State transitions:
 *   idle -> requested -> announcing -> finalizing
 *
 * Codifies invariants M2 and M3 in .claude/skills/s-arch/invariants.md.
 * Allowlisted for sync IO: owns the final `execSync('kill -9 ...')` that
 * ends the process after cleanup (Electron 41 patches process.exit /
 * app.exit inside before-quit, so the shell kill is the only exit path).
 */
import { app, globalShortcut } from 'electron';
import { execSync } from 'child_process';
import log from '#main/global/providers/logger';
import { cleanupDaemon, setIsQuitting } from '#main/global/runtime/lifecycle';
import { cleanupAutoUpdate } from '#main/updater/runtime/updater';
import { announceUiQuittingSync } from '#main/global/runtime/uiLifecycle';

export type QuitPhase = 'idle' | 'requested' | 'announcing' | 'finalizing';

interface QuitRuntime {
  phase: QuitPhase;
}

const runtime: QuitRuntime = { phase: 'idle' };

export function quitPhase(): QuitPhase {
  return runtime.phase;
}

export function isQuitRequested(): boolean {
  return runtime.phase !== 'idle';
}

export function requestQuit(): void {
  if (runtime.phase === 'idle') {
    runtime.phase = 'requested';
    setIsQuitting(true);
  }
  app.quit();
}

export interface BeforeQuitHandlers {
  stopRendererServer: () => void;
}

/**
 * Drives `idle/requested -> announcing -> finalizing`. Called from the
 * `app.on('before-quit')` handler. Notifies the daemon synchronously so
 * it sets `desired_state = Suppressed` and doesn't respawn us, tears
 * down subsystems, and force-exits the process.
 */
export function runBeforeQuit(handlers: BeforeQuitHandlers): void {
  runtime.phase = 'requested';
  setIsQuitting(true);

  runtime.phase = 'announcing';
  try {
    announceUiQuittingSync();
  } catch (err) {
    // Best-effort; daemon will stop respawning after 5 consecutive failures.
    log.warn('[quit] announceUiQuittingSync failed:', err);
  }

  runtime.phase = 'finalizing';
  globalShortcut.unregisterAll();
  cleanupDaemon();
  cleanupAutoUpdate();
  handlers.stopRendererServer();

  // Electron 41 patches process.kill / app.exit / process.exit so they
  // don't terminate inside before-quit. External kill via shell is the
  // only way to actually end the process.
  execSync(`kill -9 ${process.pid}`, { stdio: 'ignore' });
}
