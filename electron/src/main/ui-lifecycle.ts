import log from '#main/logger';
import { daemonFetch } from '#main/daemon';

const UI_ENDPOINT_TIMEOUT_MS = 1500;

type UiTransition = 'register' | 'quitting' | 'updating' | 'launch' | 'restart';

function uiLaunchEnv(): Record<string, string> {
  const keep = new Set([
    'MANDO_APP_MODE',
    'MANDO_DATA_DIR',
    'MANDO_EXTERNAL_GATEWAY',
    'MANDO_GATEWAY_PORT',
    'MANDO_LOG_DIR',
    'MANDO_HEADLESS',
    'MANDO_SANDBOX_VISIBLE',
    'ELECTRON_DISABLE_SECURITY_WARNINGS',
    'VITE_DEV_SERVER_URL',
  ]);

  return Object.entries(process.env).reduce<Record<string, string>>((env, [key, value]) => {
    if (keep.has(key) && typeof value === 'string' && value.length > 0) {
      env[key] = value;
    }
    return env;
  }, {});
}

async function postUiTransition(transition: UiTransition, body?: unknown): Promise<boolean> {
  try {
    const resp = await daemonFetch(`/api/ui/${transition}`, {
      method: 'POST',
      keepalive: true,
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(UI_ENDPOINT_TIMEOUT_MS),
    });
    if (resp.ok) return true;

    if (resp.status === 404) {
      log.debug(`[ui] /api/ui/${transition} not available yet`);
      return false;
    }

    const detail = await resp.text().catch(() => '');
    log.warn(
      `[ui] /api/ui/${transition} failed: HTTP ${resp.status}${
        detail ? ` ${detail.slice(0, 120)}` : ''
      }`,
    );
    return false;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    log.debug(`[ui] /api/ui/${transition} request failed: ${message}`);
    return false;
  }
}

export function announceUiRegistered(): Promise<boolean> {
  return postUiTransition('register', {
    pid: process.pid,
    execPath: process.execPath,
    args: process.argv.slice(1),
    cwd: process.cwd(),
    env: uiLaunchEnv(),
  });
}

export function announceUiQuitting(): Promise<boolean> {
  return postUiTransition('quitting');
}

export function announceUiUpdating(): Promise<boolean> {
  return postUiTransition('updating');
}
