import { getGatewayUrl } from '#renderer/global/providers/native/app';
import { resolveRoutePath, type StaticRouteOptions } from '#shared/daemon-contract/runtime';
import type { StaticRouteKey } from '#shared/daemon-contract/routes';

function createHttpBaseState() {
  let baseUrl = 'http://127.0.0.1:18893';

  return {
    async init(): Promise<void> {
      const url = await getGatewayUrl();
      if (url) baseUrl = url;
    },
    buildUrl(path: string): string {
      return `${baseUrl}${path}`;
    },
  };
}

const httpBaseState = createHttpBaseState();

export async function initBaseUrl(): Promise<void> {
  await httpBaseState.init();
}

export function buildUrl(path: string): string {
  return httpBaseState.buildUrl(path);
}

export function staticRoutePath<K extends StaticRouteKey>(
  key: K,
  options?: StaticRouteOptions<K>,
): string {
  return resolveRoutePath(key, options);
}
