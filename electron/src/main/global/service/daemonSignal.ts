import { execFileSync } from 'child_process';
import {
  assertRouteBody,
  resolveRoutePath,
  type JsonRouteOptions,
  type RouteBody,
} from '#shared/daemon-contract/runtime';
import {
  routes as contractRoutes,
  type MutationJsonRouteWithResKey,
  type Routes,
} from '#shared/daemon-contract/routes';
import { readPortSync, readTokenSync } from '#main/global/service/daemonDiscovery';

export function daemonRouteSignal<K extends MutationJsonRouteWithResKey>(
  key: K,
  routeOptions?: JsonRouteOptions<K>,
  options?: {
    method?: Routes[K]['method'];
    timeoutMs?: number;
    body?: RouteBody<K>;
    headers?: Record<string, string>;
  },
): void {
  const port = process.env.MANDO_GATEWAY_PORT || readPortSync();
  const token = process.env.MANDO_AUTH_TOKEN || readTokenSync();
  const method = options?.method ?? contractRoutes[key].method;
  const headers: Record<string, string> = {
    Authorization: `Bearer ${token}`,
    ...(options?.headers ?? {}),
  };
  const args = ['-sf', '-X', method];
  assertRouteBody(key, options?.body);

  for (const [name, value] of Object.entries(headers)) {
    args.push('-H', `${name}: ${value}`);
  }

  if (options?.body !== undefined) {
    if (!headers['Content-Type']) {
      args.push('-H', 'Content-Type: application/json');
    }
    args.push('--data-binary', JSON.stringify(options.body));
  }

  args.push(`http://127.0.0.1:${port}${resolveRoutePath(key, routeOptions)}`);

  execFileSync('curl', args, {
    timeout: options?.timeoutMs ?? 2000,
    stdio: 'ignore',
  });
}
