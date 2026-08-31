import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/daemon-sync-policy.mjs';

ruleTester.run('architecture/daemon-sync-policy', rule, {
  valid: [
    {
      filename: 'src/renderer/domains/captain/repo/queries.ts',
      code: `useQuery<Result>({
        queryKey,
        meta: daemonSyncMeta('sse-patched', 'task events'),
        queryFn: () => toReactQuery(fetchTasks()),
      });`,
    },
    {
      filename: 'src/renderer/domains/settings/repo/queries.ts',
      code: `useQuery({
        queryKey,
        meta: daemonSyncMeta('polling', 'health changes independently'),
        queryFn: () => toReactQuery(apiGetRouteR('getHealthTelegram')),
        refetchInterval: 10_000,
      });`,
    },
    {
      filename: 'src/renderer/domains/onboarding/repo/queries.ts',
      code: `useQuery({ queryKey, queryFn: () => checkClaudeCodeNative() });`,
    },
    {
      filename: 'src/renderer/global/repo/syncPolicy.ts',
      code: `client.invalidateQueries();`,
    },
    {
      filename: 'src/renderer/global/repo/configMutations.ts',
      code: `client.invalidateQueries({ queryKey });`,
    },
  ],
  invalid: [
    {
      filename: 'src/renderer/domains/captain/repo/queries.ts',
      code: `useQuery({ queryKey, queryFn: () => toReactQuery(fetchTasks()) });`,
      errors: [{ messageId: 'missingMeta' }],
    },
    {
      filename: 'src/renderer/domains/settings/repo/queries.ts',
      code: `useQuery({ queryKey, queryFn: () => apiGetRouteR('getHealthTelegram') });`,
      errors: [{ messageId: 'missingMeta' }],
    },
    {
      filename: 'src/renderer/domains/settings/repo/queries.ts',
      code: `useQuery({
        queryKey,
        meta: daemonSyncMeta('manual'),
        queryFn: () => toReactQuery(fetchHealth()),
        refetchInterval: 10_000,
      });`,
      errors: [{ messageId: 'pollingMeta' }],
    },
    {
      filename: 'src/renderer/global/runtime/sseEventRouter.ts',
      code: `client.invalidateQueries();`,
      errors: [{ messageId: 'bareInvalidate' }],
    },
  ],
});
