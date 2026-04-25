import type { OnBeforeSendHeadersListenerDetails } from 'electron/main';
import { session } from 'electron';
import { isTrustedRendererUrl } from '#main/global/runtime/ipcSecurity';
import { passthrough, sourceUrl } from '#main/daemon/service/gatewayAuth';
import log from '#main/global/providers/logger';
import { readPort, readToken } from '#main/global/runtime/lifecycle';

let authHookInstalled = false;

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

    let gatewayPort = process.env.MANDO_GATEWAY_PORT;
    if (!gatewayPort) {
      try {
        gatewayPort = await readPort();
      } catch (err: unknown) {
        log.debug('[gateway-auth] readPort failed, skipping auth injection:', err);
        gatewayPort = '';
      }
    }
    if (!gatewayPort || requestUrl.port !== gatewayPort) {
      callback(passthrough(details));
      return;
    }

    let token = process.env.MANDO_AUTH_TOKEN;
    if (!token) {
      try {
        token = await readToken();
      } catch (err: unknown) {
        log.debug('[gateway-auth] readToken failed, skipping auth injection:', err);
        token = undefined;
      }
    }
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
