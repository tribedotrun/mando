/**
 * IPC handlers for config read/write and onboarding setup-complete flow.
 */
import fs from 'fs';
import path from 'path';
import type { IpcMainInvokeEvent } from 'electron';
import { shell } from 'electron';
import log from '#main/logger';
import { handleTrusted } from '#main/ipc-security';
import { installCliAndPlists, getDaemonStatus } from '#main/launchd';
import { getDataDir, getConfigPath, daemonFetch, ensureDaemon, getAppMode } from '#main/daemon';

class DaemonConfigHttpError extends Error {}

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

  handleTrusted('save-config', async (_: unknown, configJson: string) => {
    try {
      const resp = await daemonFetch('/api/config', {
        method: 'PUT',
        body: configJson,
      });
      if (resp.ok) return true;
      const err = await resp.json().catch(() => ({ error: resp.statusText }));
      log.error('save-config via daemon failed:', err);
      throw new DaemonConfigHttpError(err.error || `HTTP ${resp.status}`);
    } catch (e: unknown) {
      if (e instanceof DaemonConfigHttpError) {
        throw e;
      }

      const message = e instanceof Error ? e.message : String(e);
      const networkFallback =
        message.includes('fetch failed') ||
        message.includes('ECONNREFUSED') ||
        message.includes('ENOTFOUND') ||
        message.includes('timed out');

      if (!networkFallback) {
        throw e;
      }

      log.error('save-config fetch failed:', message);
    }
    // Fallback: write locally so config isn't lost.
    log.warn('save-config: daemon unreachable, falling back to local file write');
    const configPath = getConfigPath();
    fs.mkdirSync(path.dirname(configPath), { recursive: true });
    fs.writeFileSync(configPath, configJson, 'utf-8');
    return true;
  });

  /** Save partial onboarding progress to a separate file (not config.json — that would make hasConfig return true). */
  handleTrusted('save-config-local', (_: unknown, configJson: string) => {
    const dir = path.dirname(getConfigPath());
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'config.partial.json'), configJson, 'utf-8');
    return true;
  });

  handleTrusted('setup-complete', async (event: IpcMainInvokeEvent, configJson: string) => {
    const send = (step: string) => event.sender.send('setup-progress', step);
    const dataDir = getDataDir();

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
    let setupNotified = false;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        await daemonFetch('/api/config/setup', {
          method: 'POST',
          body: JSON.stringify({ config: JSON.parse(configJson) }),
        });
        setupNotified = true;
        break;
      } catch (err: unknown) {
        log.debug(`setup-complete: attempt ${attempt + 1}/5 failed:`, err);
        await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)));
      }
    }
    if (!setupNotified) {
      log.error(
        'setup-complete: failed to notify daemon after 5 attempts — daemon will pick up config on next restart, but captain may not start until then',
      );
    }

    if (getAppMode() !== 'sandbox') {
      send('Installing CLI\u2026');
      try {
        installCliAndPlists(dataDir);
      } catch (e: unknown) {
        log.warn('launchd setup failed:', e instanceof Error ? e.message : e);
      }
    }
    return true;
  });

  // -- Launchd IPC handlers (Electron-native) --
  handleTrusted('add-project', async (_: IpcMainInvokeEvent, bodyJson: string) => {
    let resp: Awaited<ReturnType<typeof daemonFetch>>;
    try {
      resp = await daemonFetch('/api/projects', {
        method: 'POST',
        body: bodyJson,
      });
    } catch (err: unknown) {
      log.error('[add-project] daemon unreachable:', err);
      throw new Error('Daemon is not running. Start the daemon and try again.', { cause: err });
    }
    const data = (await resp.json()) as Record<string, unknown>;
    if (!resp.ok) {
      throw new Error((data.error as string) || `HTTP ${resp.status}`);
    }
    return data;
  });

  handleTrusted('launchd:reinstall', () => {
    installCliAndPlists(getDataDir());
    return true;
  });
  handleTrusted('launchd:daemon-status', () => getDaemonStatus());
  handleTrusted('open-logs-folder', () => shell.openPath(path.join(getDataDir(), 'logs')));
}
