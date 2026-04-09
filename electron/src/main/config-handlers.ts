/**
 * IPC handlers for config read/write and onboarding setup-complete flow.
 */
import fs from 'fs';
import path from 'path';
import { spawn } from 'child_process';
import { shell, type IpcMainInvokeEvent } from 'electron';
import log from '#main/logger';
import { handleTrusted } from '#main/ipc-security';
import { installCliAndPlists } from '#main/launchd';
import { getDataDir, getConfigPath, daemonFetch, ensureDaemon, getAppMode } from '#main/daemon';

export function registerConfigHandlers(): void {
  handleTrusted('has-config', async () => {
    try {
      const resp = await daemonFetch('/api/config/status');
      if (resp.ok) {
        const data = (await resp.json()) as { setupComplete: boolean };
        return data.setupComplete;
      }
    } catch (e: unknown) {
      log.debug('[has-config] daemon check failed:', e);
    }
    const configPath = getConfigPath();
    if (!fs.existsSync(configPath)) return false;
    try {
      const raw = fs.readFileSync(configPath, 'utf-8');
      return typeof JSON.parse(raw) === 'object';
    } catch (e: unknown) {
      log.warn('[has-config] local config parse failed:', e);
      return false;
    }
  });

  handleTrusted('read-config', async () => {
    try {
      const resp = await daemonFetch('/api/config');
      if (resp.ok) return await resp.text();
    } catch (err: unknown) {
      log.debug('read-config: daemon not ready, falling back to local file:', err);
    }
    try {
      return fs.readFileSync(getConfigPath(), 'utf-8');
    } catch (err: unknown) {
      log.error('read-config: both daemon and local file read failed:', err);
      throw new Error('Failed to load config from daemon and local file', { cause: err });
    }
  });

  // save-config IPC removed — renderer calls PUT /api/config directly

  // Save partial onboarding progress to a separate file. Using config.json
  // would make hasConfig return true and skip the remainder of onboarding.
  handleTrusted('save-config-local', (_: unknown, configJson: string) => {
    try {
      const dir = path.dirname(getConfigPath());
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, 'config.partial.json'), configJson, 'utf-8');
      return true;
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      log.error('save-config-local: failed to persist partial config:', message);
      throw new Error(`Failed to save onboarding progress: ${message}`, { cause: err });
    }
  });

  handleTrusted('setup-complete', async (event: IpcMainInvokeEvent, configJson: string) => {
    const send = (step: string) => event.sender.send('setup-progress', step);
    const dataDir = getDataDir();
    let lastError: string | undefined;

    send('Saving configuration\u2026');
    for (const sub of ['state', 'logs', 'images']) {
      fs.mkdirSync(path.join(dataDir, sub), { recursive: true });
    }
    const configPath = getConfigPath();
    fs.mkdirSync(path.dirname(configPath), { recursive: true });
    fs.writeFileSync(configPath, configJson, 'utf-8');
    const partial = path.join(path.dirname(configPath), 'config.partial.json');
    if (fs.existsSync(partial)) fs.unlinkSync(partial);

    send('Starting daemon\u2026');
    await ensureDaemon(dataDir);

    send('Configuring daemon\u2026');
    let daemonNotified = false;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        await daemonFetch('/api/config/setup', {
          method: 'POST',
          body: JSON.stringify({ config: JSON.parse(configJson) }),
        });
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

  // add-project IPC removed — renderer calls POST /api/projects directly
  // launchd:reinstall IPC removed — was never called from renderer

  handleTrusted('open-logs-folder', () => shell.openPath(path.join(getDataDir(), 'logs')));
  handleTrusted('open-data-dir', () => shell.openPath(getDataDir()));
  handleTrusted('open-config-file', () => shell.openPath(getConfigPath()));
  handleTrusted('open-in-finder', async (_e, dir: string) => {
    const err = await shell.openPath(dir);
    if (err) {
      log.warn(`open-in-finder failed for "${dir}": ${err}`);
      throw new Error(err);
    }
  });
  handleTrusted('open-in-cursor', (_e, dir: string) => {
    try {
      spawn('cursor', [dir], { detached: true, stdio: 'ignore' }).unref();
    } catch (err) {
      log.warn('open-in-cursor failed:', err);
      throw err;
    }
  });
}
