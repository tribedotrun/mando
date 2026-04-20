import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/require-result-return.mjs';

ruleTester.run('mando/require-result-return', rule, {
  valid: [
    // Outside TIER_RE
    {
      filename: '/r/electron/src/renderer/global/types/index.ts',
      code: `async function f(): Promise<string> { return 'x'; }`,
    },
    // *.tsx allowlist
    {
      filename: '/r/electron/src/renderer/domains/captain/repo/foo.tsx',
      code: `async function f(): Promise<string> { return 'x'; }`,
    },
    // Result-like return type
    {
      filename: '/r/electron/src/renderer/domains/scout/repo/api.ts',
      code: `import type { ResultAsync, ApiError } from '#result';
        function f(): ResultAsync<string, ApiError> { return undefined as never; }`,
    },
    // Promise<Result<T, E>>
    {
      filename: '/r/electron/src/renderer/domains/scout/repo/api.ts',
      code: `async function f(): Promise<Result<string, ApiError>> { return undefined as never; }`,
    },
    // No explicit return type — not flagged (avoid false positives without type info)
    {
      filename: '/r/electron/src/renderer/domains/scout/repo/api.ts',
      code: `async function f() { return 'x'; }`,
    },
    // Promise<void> is fine — side-effect only
    {
      filename: '/r/electron/src/renderer/domains/scout/service/foo.ts',
      code: `async function f(): Promise<void> { await doSomething(); }`,
    },
    // React Query: queryFn value inside useQuery -- library contract, must return bare Promise<T>
    {
      filename: '/r/electron/src/renderer/global/runtime/useConfig.ts',
      code: `
        declare function useQuery(opts: object): unknown;
        export function useConfig() {
          return useQuery({
            queryKey: ['config'],
            queryFn: async (): Promise<string> => { return fetch('/api').then(r => r.json()); },
          });
        }
      `,
    },
    // React Query: mutationFn value inside useMutation
    {
      filename: '/r/electron/src/renderer/domains/captain/runtime/hooks.ts',
      code: `
        declare function useMutation(opts: object): unknown;
        export function useDoThing() {
          return useMutation({
            mutationFn: async (id: number): Promise<string> => { return fetch('/api/' + id).then(r => r.json()); },
          });
        }
      `,
    },
    // React Query: queryFn with useInfiniteQuery
    {
      filename: '/r/electron/src/renderer/domains/scout/runtime/useList.ts',
      code: `
        declare function useInfiniteQuery(opts: object): unknown;
        export function useList() {
          return useInfiniteQuery({
            queryKey: ['list'],
            queryFn: async ({ pageParam }: { pageParam: number }): Promise<string[]> => {
              return fetch('/api?page=' + pageParam).then(r => r.json());
            },
          });
        }
      `,
    },
    // invariant annotation on a plain function declaration
    {
      filename: '/r/electron/src/renderer/global/runtime/highlighter.ts',
      code: `
        // invariant: singleton promise factory; callers receive the live instance
        export function getHighlighter(): Promise<Highlighter> { return Promise.resolve({} as Highlighter); }
      `,
    },
    // invariant annotation on an arrow function in a variable declaration
    {
      filename: '/r/electron/src/renderer/global/runtime/useNativeActions.ts',
      code: `
        declare function useCallback<T>(fn: T, deps: unknown[]): T;
        export function useNativeActions() {
          // invariant: IPC passthrough; null means dialog dismissed
          const selectDir = useCallback(async (): Promise<string | null> => null, []);
          return { selectDir };
        }
      `,
    },
    // invariant annotation on a useCallback arrow function (walks up past CallExpression to VariableDeclaration)
    {
      filename: '/r/electron/src/renderer/domains/onboarding/runtime/useSetupIpc.ts',
      code: `
        declare function useCallback<T>(fn: T, deps: unknown[]): T;
        export function useSetupIpc() {
          // invariant: IPC passthrough; null means dialog dismissed
          const selectDirectory = useCallback(async (): Promise<string | null> => {
            return window.mandoAPI.selectDirectory();
          }, []);
          return { selectDirectory };
        }
      `,
    },
    // Service-tier file -- valid: coverage now extends to service/
    {
      filename: '/r/electron/src/renderer/global/service/utils.ts',
      code: `
        // invariant: clipboard result encoded as boolean; errors absorbed via toast
        export async function copyToClipboard(text: string): Promise<boolean> {
          try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
        }
      `,
    },
    // Runtime-tier file -- valid: useCallback with invariant
    {
      filename: '/r/electron/src/renderer/domains/onboarding/runtime/useTelegramTokenValidator.ts',
      code: `
        declare function useCallback<T>(fn: T, deps: unknown[]): T;
        export function useTelegramTokenValidator() {
          // invariant: errors absorbed as false return
          const validate = useCallback(async (token: string): Promise<boolean> => {
            return true;
          }, []);
          return { validate };
        }
      `,
    },
  ],
  invalid: [
    // repo-tier bare Promise<T> (original coverage)
    {
      filename: '/r/electron/src/renderer/domains/scout/repo/api.ts',
      code: `async function f(): Promise<string> { return 'x'; }`,
      errors: [{ messageId: 'bare' }],
    },
    {
      filename: '/r/electron/src/renderer/domains/captain/repo/foo.ts',
      code: `function f(): Promise<number> { return Promise.resolve(1); }`,
      errors: [{ messageId: 'bare' }],
    },
    // service-tier bare Promise<T> -- newly enforced
    {
      filename: '/r/electron/src/renderer/global/service/utils.ts',
      code: `export async function fetchThing(): Promise<string> { return 'x'; }`,
      errors: [{ messageId: 'bare' }],
    },
    // runtime-tier bare Promise<T> -- newly enforced
    {
      filename: '/r/electron/src/renderer/global/runtime/highlighter.ts',
      code: `export async function getSomething(): Promise<string> { return 'x'; }`,
      errors: [{ messageId: 'bare' }],
    },
    // providers-tier bare Promise<T> -- newly enforced
    {
      filename: '/r/electron/src/main/global/providers/customProvider.ts',
      code: `export async function provideThing(): Promise<string> { return 'x'; }`,
      errors: [{ messageId: 'bare' }],
    },
    // async method in a class inside runtime -- newly enforced
    {
      filename: '/r/electron/src/renderer/global/runtime/SomeManager.ts',
      code: `
        class Mgr {
          async load(): Promise<string> { return 'x'; }
        }
        export default Mgr;
      `,
      errors: [{ messageId: 'bare' }],
    },
    // queryFn in a non-React-Query call expression -- still flagged
    {
      filename: '/r/electron/src/renderer/domains/captain/runtime/notAHook.ts',
      code: `
        declare function registerHandler(opts: object): void;
        export function setup() {
          registerHandler({
            queryFn: async (): Promise<string> => { return 'x'; },
          });
        }
      `,
      errors: [{ messageId: 'bare' }],
    },
  ],
});
