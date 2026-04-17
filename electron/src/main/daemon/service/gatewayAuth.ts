import type { OnBeforeSendHeadersListenerDetails } from 'electron/main';

export function passthrough(details: OnBeforeSendHeadersListenerDetails) {
  return { requestHeaders: details.requestHeaders };
}

export function sourceUrl(details: OnBeforeSendHeadersListenerDetails): string {
  return details.frame?.url ?? details.webContents?.getURL?.() ?? details.referrer ?? '';
}
