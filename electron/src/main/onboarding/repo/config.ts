/**
 * IPC handlers for config read/write and onboarding setup-complete flow.
 */
import fs from 'fs';
import path from 'path';
import { spawn } from 'child_process';
import { shell, type IpcMainInvokeEvent } from 'electron';
import log from '#main/global/providers/logger';
import { handleChannel, sendChannel } from '#main/global/runtime/ipcSecurity';
import { installCliAndPlists } from '#main/global/runtime/launchd';
import { daemonRouteFetch, daemonRouteJsonR, ensureDaemon } from '#main/global/runtime/lifecycle';
import {
  parseConfigJsonText,
  requireConfigJsonText,
  requireValidConfigJsonText,
} from '#shared/daemon-contract/json';
import { getDataDir, getConfigPath, getAppMode } from '#main/global/config/lifecycle';

function parseConfigJson(configJson: string, where: string) {
  return requireConfigJsonText(configJson, where);
}

export function registerConfigHandlers(): void {
  handleChannel('has-config', async () => {
    const result = await daemonRouteJsonR('getConfigStatus');
    if (result.isOk()) return result.value.setupComplete;
    log.debug('[has-config] daemon check failed:', result.error);
    const configPath = getConfigPath();
    if (!fs.existsSync(configPath)) return false;
    try {
      const raw = fs.readFileSync(configPath, 'utf-8');
      return parseConfigJsonText(raw, 'ipc:has-config local config').isOk();
    } catch (e: unknown) {
      log.warn('[has-config] local config parse failed:', e);
      return false;
    }
  });

  handleChannel('read-config', async () => {
    try {
      const resp = await daemonRouteFetch('getConfig');
      if (resp.ok) {
        return requireValidConfigJsonText(await resp.text(), 'ipc:read-config daemon');
      }
    } catch (err: unknown) {
      log.debug('read-config: daemon not ready, falling back to local file:', err);
    }
    try {
      return requireValidConfigJsonText(
        fs.readFileSync(getConfigPath(), 'utf-8'),
        'ipc:read-config local',
      );
    } catch (err: unknown) {
      log.error('read-config: both daemon and local file read failed:', err);
      throw new Error('Failed to load config from daemon and local file', { cause: err });
    }
  });

  // save-config IPC removed; renderer now writes config through the typed daemon contract

  // Save partial onboarding progress to a separate file. Using config.json
  // would make hasConfig return true and skip the remainder of onboarding.
  handleChannel('save-config-local', (_event, configJson: string) => {
    try {
      const config = parseConfigJson(configJson, 'ipc:save-config-local');
      const dir = path.dirname(getConfigPath());
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(
        path.join(dir, 'config.partial.json'),
        JSON.stringify(config, null, 2),
        'utf-8',
      );
      return true;
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      log.error('save-config-local: failed to persist partial config:', message);
      throw new Error(`Failed to save onboarding progress: ${message}`, { cause: err });
    }
  });

  handleChannel('setup-complete', async (event: IpcMainInvokeEvent, configJson: string) => {
    const send = (step: string) => sendChannel(event.sender, 'setup-progress', step);
    const dataDir = getDataDir();
    let lastError: string | undefined;
    const config = parseConfigJson(configJson, 'ipc:setup-complete');
    const serializedConfig = JSON.stringify(config, null, 2);

    send('Saving configuration\u2026');
    for (const sub of ['state', 'logs', 'images']) {
      fs.mkdirSync(path.join(dataDir, sub), { recursive: true });
    }
    const configPath = getConfigPath();
    fs.mkdirSync(path.dirname(configPath), { recursive: true });
    fs.writeFileSync(configPath, serializedConfig, 'utf-8');
    const partial = path.join(path.dirname(configPath), 'config.partial.json');
    if (fs.existsSync(partial)) fs.unlinkSync(partial);

    send('Starting daemon\u2026');
    await ensureDaemon(dataDir);

    send('Configuring daemon\u2026');
    let daemonNotified = false;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        const resp = await daemonRouteFetch('postConfigSetup', undefined, {
          method: 'POST',
          body: { config },
        });
        if (!resp.ok) {
          const detail = await resp.text().catch(() => '');
          throw new Error(
            `postConfigSetup failed: HTTP ${resp.status}${detail ? ` ${detail.slice(0, 120)}` : ''}`,
          );
        }
        daemonNotified = true;
        break;
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        lastError = message;
        log.debug(`setup-complete: attempt ${attempt + 1}/5 failed: ${message}`);
        await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)));
      }
    }
    if (!daemonNotified) {
      log.error(
        `setup-complete: failed to notify daemon after 5 attempts (last: ${lastError ?? 'unknown'}) - daemon will pick up config on next restart, but captain may not start until then`,
      );
    }

    let launchdInstalled = true;
    if (getAppMode() !== 'sandbox') {
      send('Installing CLI\u2026');
      try {
        installCliAndPlists(dataDir, { skipDaemonPlist: true });
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        log.error('launchd setup failed:', message);
        launchdInstalled = false;
        if (!lastError) lastError = `Launchd install failed: ${message}`;
      }
    }
    return {
      ok: daemonNotified && launchdInstalled,
      daemonNotified,
      launchdInstalled,
      error: lastError,
    };
  });

  // add-project IPC removed; renderer now creates projects through the typed daemon contract
  // launchd:reinstall IPC removed — was never called from renderer

  handleChannel('open-logs-folder', async () => {
    await shell.openPath(path.join(getDataDir(), 'logs'));
  });
  handleChannel('open-data-dir', async () => {
    await shell.openPath(getDataDir());
  });
  handleChannel('open-config-file', async () => {
    await shell.openPath(getConfigPath());
  });
  handleChannel('open-in-finder', async (_event, dir) => {
    const err = await shell.openPath(dir);
    if (err) {
      log.warn(`open-in-finder failed for "${dir}": ${err}`);
      throw new Error(err);
    }
  });
  handleChannel('open-in-cursor', (_event, dir) => {
    try {
      spawn('cursor', [dir], { detached: true, stdio: 'ignore' }).unref();
    } catch (err) {
      log.warn('open-in-cursor failed:', err);
      throw err;
    }
  });
}
