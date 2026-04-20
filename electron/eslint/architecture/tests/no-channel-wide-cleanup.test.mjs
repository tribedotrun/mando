import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-channel-wide-cleanup.mjs';

ruleTester.run('architecture/no-channel-wide-cleanup', rule, {
  valid: [
    {
      code: `const dispose = api.onShortcut(handler); dispose();`,
    },
    {
      code: `interface API { onShortcut(cb: () => void): () => void; }`,
    },
    {
      code: `class Foo { remove(name: string) {} }`,
    },
    // Singular `removeListener(handler)` is a legitimate per-handler API
    // (e.g. EventEmitter) and must not trigger the channel-wide ban.
    {
      code: `emitter.removeListener('evt', handler);`,
    },
    {
      code: `interface Emitter { removeListener(name: string, handler: () => void): void; }`,
    },
  ],
  invalid: [
    {
      code: `api.removeShortcutListeners();`,
      errors: [{ messageId: 'noCleanupCall' }],
    },
    {
      code: `api.removeAllListeners();`,
      errors: [{ messageId: 'noCleanupCall' }],
    },
    {
      code: `api.removeUpdateListeners();`,
      errors: [{ messageId: 'noCleanupCall' }],
    },
    {
      code: `interface API { removeShortcutListeners(): void; }`,
      errors: [{ messageId: 'noCleanupDecl' }],
    },
    {
      code: `interface API { removeShortcutListeners: () => void; }`,
      errors: [{ messageId: 'noCleanupDecl' }],
    },
    {
      code: `const api = { removeShortcutListeners: () => {} };`,
      errors: [{ messageId: 'noCleanupDecl' }],
    },
  ],
});
