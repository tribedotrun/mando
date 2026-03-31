import type { OnBeforeSendHeadersListenerDetails } from 'electron/main';
import { session } from 'electron';
import { isTrustedRendererUrl } from '#main/ipc-security';
import log from '#main/logger';
import { readPort, readToken } from '#main/daemon';

let authHookInstalled = false;

function callbackHeaders(details: OnBeforeSendHeadersListenerDetails): {
  requestHeaders: Record<string, string | string[]>;
} {
  return {
    requestHeaders: details.requestHeaders,
  };
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
    if (requestUrl.protocol !== 'http:') {
      callback(callbackHeaders(details));
      return;
    }
    if (requestUrl.pathname === '/api/health') {
      callback(callbackHeaders(details));
      return;
    }
    if (!requestUrl.pathname.startsWith('/api/')) {
      callback(callbackHeaders(details));
      return;
    }

    const frameUrl = sourceUrl(details);
    if (!isTrustedRendererUrl(frameUrl)) {
      callback(callbackHeaders(details));
      return;
    }

    const gatewayPort = process.env.MANDO_GATEWAY_PORT || (await readPort().catch(() => ''));
    if (!gatewayPort || requestUrl.port !== gatewayPort) {
      callback(callbackHeaders(details));
      return;
    }

    const token = process.env.MANDO_AUTH_TOKEN || (await readToken().catch(() => null));
    if (!token) {
      callback(callbackHeaders(details));
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
    callback(callbackHeaders(details));
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
