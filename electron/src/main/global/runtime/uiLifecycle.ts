import log from '#main/global/providers/logger';
import { daemonRouteFetch, daemonRouteSignal } from '#main/global/runtime/lifecycle';
import { uiLaunchEnv } from '#main/global/service/uiLifecycle';
import type { RouteBody } from '#shared/daemon-contract/runtime';

const UI_ENDPOINT_TIMEOUT_MS = 1500;

type UiTransition = 'register' | 'quitting' | 'updating' | 'launch' | 'restart';
const UI_ROUTE_KEYS = {
  register: 'postUiRegister',
  quitting: 'postUiQuitting',
  updating: 'postUiUpdating',
  launch: 'postUiLaunch',
  restart: 'postUiRestart',
} as const;

async function postUiTransition<T extends UiTransition>(
  transition: T,
  body?: RouteBody<(typeof UI_ROUTE_KEYS)[T]>,
): Promise<boolean> {
  try {
    const resp = await daemonRouteFetch(UI_ROUTE_KEYS[transition], undefined, {
      method: 'POST',
      keepalive: true,
      body,
      signal: AbortSignal.timeout(UI_ENDPOINT_TIMEOUT_MS),
    });
    if (resp.ok) return true;

    if (resp.status === 404) {
      log.debug(`[ui] /api/ui/${transition} not available yet`);
      return false;
    }

    let detail = '';
    try {
      detail = await resp.text();
    } catch {
      detail = '';
    }
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
  return postUiTransition('updating', {});
}

export function announceUiQuittingSync(): void {
  daemonRouteSignal('postUiQuitting', undefined, { body: {} });
}
