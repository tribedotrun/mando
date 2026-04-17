import log from '#main/global/providers/logger';
import { daemonFetch } from '#main/global/runtime/lifecycle';
import { uiLaunchEnv } from '#main/global/service/uiLifecycle';

const UI_ENDPOINT_TIMEOUT_MS = 1500;

type UiTransition = 'register' | 'quitting' | 'updating' | 'launch' | 'restart';

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

export function announceUiUpdating(): Promise<boolean> {
  return postUiTransition('updating');
}
