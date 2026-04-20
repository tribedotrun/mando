import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-bare-throw.mjs';

ruleTester.run('mando/no-bare-throw', rule, {
  valid: [
    // *.tsx allowlist (React render path)
    {
      filename: '/r/electron/src/renderer/foo.tsx',
      code: `function f() { throw new Error('boom'); }`,
    },
    // Files in shared/result/ allowlist
    {
      filename: '/r/electron/src/shared/result/helpers.ts',
      code: `function f() { throw new Error('boom'); }`,
    },
    // Throw allowed in funnel files
    {
      filename: '/r/electron/src/main/global/runtime/lifecycle.ts',
      code: `function f() { throw new Error('boom'); }`,
    },
    // // invariant: comment escape hatch
    {
      filename: '/r/electron/src/renderer/domains/captain/service/foo.ts',
      code: `function f() {
        // invariant: caller must check x first
        throw new Error('unreachable');
      }`,
    },
  ],
  invalid: [
    {
      filename: '/r/electron/src/renderer/domains/captain/service/foo.ts',
      code: `function f() { throw new Error('boom'); }`,
      errors: [{ messageId: 'bare' }],
    },
  ],
});
