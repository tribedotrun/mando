/**
 * Owner for the system tray (macOS menu bar). Single authority for tray
 * creation, menu construction, and the "tray failed to create" fallback
 * that the window close handler checks before hiding vs quitting.
 *
 * Codifies invariants M1 and M2 in .claude/skills/s-arch/invariants.md.
 * Allowlisted for sync IO: the "Quit All" menu item reads the daemon's
 * pid file and sends a SIGKILL, which is the only way to guarantee the
 * daemon exits before the UI does in dev/sandbox.
 */
import { Tray, Menu } from 'electron';
import fs from 'fs';
import path from 'path';
import log from '#main/global/providers/logger';
import { getAppMode, getDataDir } from '#main/global/config/lifecycle';
import { getAppTitle } from '#main/global/service/lifecycle';
import { createTrayIcon } from '#main/global/runtime/icons';
import { updateTrayTooltip } from '#main/global/runtime/lifecycle';
import { showAndFocusMainWindow, setTrayAvailable } from '#main/global/runtime/windowOwner';
import { requestQuit } from '#main/global/runtime/quitController';

interface TrayRuntime {
  tray: Tray | null;
}

const runtime: TrayRuntime = { tray: null };

export function installTray(resolveAsset: (name: string) => string): void {
  try {
    const icon = createTrayIcon(resolveAsset('trayTemplate@2x.png'), getAppMode());
    const tray = new Tray(icon);
    tray.setToolTip(updateTrayTooltip());

    const title = getAppTitle(getAppMode());
    const mode = getAppMode();
    const items: Electron.MenuItemConstructorOptions[] = [
      {
        label: `Show ${title}`,
        click: () => showAndFocusMainWindow(),
      },
      { type: 'separator' },
      {
        label: 'Quit UI',
        click: () => requestQuit(),
      },
    ];

    if (mode !== 'production') {
      items.push({
        label: 'Quit All (daemon + UI)',
        click: () => quitAllIncludingDaemon(),
      });
    }

    tray.setContextMenu(Menu.buildFromTemplate(items));
    runtime.tray = tray;
    setTrayAvailable(true);
  } catch (err) {
    // Tray creation can fail on missing icon assets or on systems without a
    // menu bar. Without a tray there is no way to reopen the app from a
    // hidden state, so the window close handler must fall back to quit.
    log.error('[main] tray creation failed, disabling close-to-tray:', err);
    setTrayAvailable(false);
  }
}

function quitAllIncludingDaemon(): void {
  const pidFile = path.join(getDataDir(), 'daemon.pid');
  let pid: number | undefined;
  try {
    pid = parseInt(fs.readFileSync(pidFile, 'utf-8').trim(), 10);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code !== 'ENOENT') {
      log.warn('[tray] failed to read daemon.pid:', err);
    }
    requestQuit();
    return;
  }
  if (!pid || Number.isNaN(pid)) {
    requestQuit();
    return;
  }

  // Existence + ownership probe. `process.kill(pid, 0)` throws ESRCH when
  // the process is gone and EPERM when it exists but belongs to another
  // user — in either case this is not our daemon and we must not
  // SIGKILL a PID-reused victim.
  try {
    process.kill(pid, 0);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code === 'ESRCH') {
      // Daemon already dead; stale pid file.
    } else if (code === 'EPERM') {
      log.warn(`[tray] pid ${pid} exists but is owned by another user; not killing`);
    } else {
      log.warn(`[tray] process.kill(${pid}, 0) failed:`, err);
    }
    requestQuit();
    return;
  }

  try {
    process.kill(pid, 'SIGKILL');
  } catch (err) {
    log.warn(`[tray] SIGKILL pid=${pid} failed:`, err);
  }
  requestQuit();
}
