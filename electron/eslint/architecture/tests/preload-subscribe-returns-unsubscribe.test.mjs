import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/preload-subscribe-returns-unsubscribe.mjs';

ruleTester.run('architecture/preload-subscribe-returns-unsubscribe', rule, {
  valid: [
    {
      code: `interface MandoAPI { onShortcut: (cb: (action: string) => void) => () => void; }`,
      filename: 'src/preload/types/api.ts',
    },
    {
      code: `interface MandoAPI { onUpdateReady(cb: () => void): () => void; }`,
      filename: 'src/preload/types/api.ts',
    },
    // Non-on* methods are not affected.
    {
      code: `interface MandoAPI { gatewayUrl: () => Promise<string | null>; }`,
      filename: 'src/preload/types/api.ts',
    },
    // The rule is scoped to preload/types/api.ts and api-channel-map.ts; other files unaffected.
    {
      code: `interface API { onShortcut: (cb: (a: string) => void) => void; }`,
      filename: 'src/renderer/global/runtime/foo.ts',
    },
  ],
  invalid: [
    {
      code: `interface MandoAPI { onShortcut: (cb: (a: string) => void) => void; }`,
      filename: 'src/preload/types/api.ts',
      errors: [{ messageId: 'mustReturnDisposer' }],
    },
    {
      code: `interface MandoAPI { onUpdateReady(cb: () => void): void; }`,
      filename: 'src/preload/types/api.ts',
      errors: [{ messageId: 'mustReturnDisposer' }],
    },
    // Disposer must return `void`, not `unknown` or `any`.
    {
      code: `interface MandoAPI { onShortcut: (cb: (a: string) => void) => () => unknown; }`,
      filename: 'src/preload/types/api.ts',
      errors: [{ messageId: 'mustReturnDisposer' }],
    },
  ],
});
