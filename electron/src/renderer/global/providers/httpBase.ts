import { resolveRoutePath, type StaticRouteOptions } from '#shared/daemon-contract/runtime';
import type { StaticRouteKey } from '#shared/daemon-contract/routes';

let baseUrl = 'http://127.0.0.1:18893';

export async function initBaseUrl(): Promise<void> {
  if (!window.mandoAPI) return;
  const url = await window.mandoAPI.gatewayUrl();
  if (url) baseUrl = url;
}

export function buildUrl(path: string): string {
  return `${baseUrl}${path}`;
}

export function staticRoutePath<K extends StaticRouteKey>(
  key: K,
  options?: StaticRouteOptions<K>,
): string {
  return resolveRoutePath(key, options);
}
