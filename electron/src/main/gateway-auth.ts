import type { OnBeforeSendHeadersListenerDetails } from 'electron/main';
import { session } from 'electron';
import { isTrustedRendererUrl } from '#main/ipc-security';
import log from '#main/logger';
import { readPort, readToken } from '#main/daemon';

let authHookInstalled = false;

function passthrough(details: OnBeforeSendHeadersListenerDetails) {
  return { requestHeaders: details.requestHeaders };
}

function sourceUrl(details: OnBeforeSendHeadersListenerDetails): string {
  return details.frame?.url ?? details.webContents?.getURL?.() ?? details.referrer ?? '';
}

async function attachGatewayAuth(
  details: OnBeforeSendHeadersListenerDetails,
  callback: (response: { requestHeaders: Record<string, string | string[]> }) => void,
): Promise<void> {
  try {
    const requestUrl = new URL(details.url);

    // Only inject auth for trusted renderer requests to gateway /api/ (excluding /api/health).
    const needsAuth =
      requestUrl.protocol === 'http:' &&
      requestUrl.pathname.startsWith('/api/') &&
      requestUrl.pathname !== '/api/health' &&
      isTrustedRendererUrl(sourceUrl(details));

    if (!needsAuth) {
      callback(passthrough(details));
      return;
    }

    const gatewayPort =
      process.env.MANDO_GATEWAY_PORT ||
      (await readPort().catch((err: unknown) => {
        log.debug('[gateway-auth] readPort failed, skipping auth injection:', err);
        return '';
      }));
    if (!gatewayPort || requestUrl.port !== gatewayPort) {
      callback(passthrough(details));
      return;
    }

    const token =
      process.env.MANDO_AUTH_TOKEN ||
      (await readToken().catch((err: unknown) => {
        log.debug('[gateway-auth] readToken failed, skipping auth injection:', err);
        return null;
      }));
    if (!token) {
      callback(passthrough(details));
      return;
    }

    callback({
      requestHeaders: {
        ...details.requestHeaders,
        Authorization: `Bearer ${token}`,
      },
    });
  } catch (err) {
    log.warn('[gateway-auth] failed to attach daemon auth header:', err);
    callback(passthrough(details));
  }
}

export function installTrustedGatewayAuth(): void {
  if (authHookInstalled) return;
  authHookInstalled = true;

  session.defaultSession.webRequest.onBeforeSendHeaders(
    {
      urls: ['http://127.0.0.1:*/*', 'http://localhost:*/*'],
    },
    (details, callback) => {
      void attachGatewayAuth(details, callback);
    },
  );
}
