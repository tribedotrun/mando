import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-module-scope-mutable-state.mjs';

ruleTester.run('architecture/no-module-scope-mutable-state', rule, {
  valid: [
    // const declarations are fine.
    {
      code: `const cache = new Map();`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
    },
    // `let` inside a function is fine.
    {
      code: `function fn() { let counter = 0; counter++; return counter; }`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
    },
    // Other files are not affected.
    {
      code: `let counter = 0;`,
      filename: 'electron/src/main/global/runtime/foo.ts',
    },
    // index.ts is covered by `main-composition-only`, not this rule.
    {
      code: `let mainWindow = null;`,
      filename: 'electron/src/main/index.ts',
    },
  ],
  invalid: [
    {
      code: `let connectionState = 'connecting';`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
      errors: [{ messageId: 'noModuleLet' }],
    },
    {
      code: `let reconnectAttempts = 0; let reconnectDelay = 1000;`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
      errors: [{ messageId: 'noModuleLet' }, { messageId: 'noModuleLet' }],
    },
    // Destructured let at module scope is still banned.
    {
      code: `let { phase } = initial;`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
      errors: [{ messageId: 'noModuleLet' }],
    },
    {
      code: `let [first, second] = pair;`,
      filename: 'electron/src/main/global/runtime/lifecycle.ts',
      errors: [{ messageId: 'noModuleLet' }],
    },
  ],
});
