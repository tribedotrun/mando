import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-direct-localStorage.mjs';

ruleTester.run('architecture/no-direct-localStorage', rule, {
  valid: [
    {
      code: `import { defineSlot } from '#renderer/global/providers/persistence';`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
    },
    {
      code: `const x = persistence.read();`,
      filename: 'src/renderer/global/runtime/foo.ts',
    },
    // The persistence module itself is the only place allowed to touch storage.
    {
      code: `localStorage.getItem('k');`,
      filename: 'src/renderer/global/providers/persistence.ts',
    },
    // Passing through a parameter named `localStorage` is allowed when not the global.
    {
      code: `function fn(obj) { return obj.localStorage; }`,
      filename: 'src/renderer/global/runtime/foo.ts',
    },
    // Shadowed local parameter `localStorage` — scope resolution proves
    // this is not the global, so no violation.
    {
      code: `function fn(localStorage) { return localStorage; }`,
      filename: 'src/renderer/global/runtime/foo.ts',
    },
    // Shadowed local const `localStorage` — also not the global.
    {
      code: `function fn() { const localStorage = {}; return localStorage; }`,
      filename: 'src/renderer/global/runtime/foo.ts',
    },
  ],
  invalid: [
    {
      code: `localStorage.setItem('k', 'v');`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
      errors: [{ messageId: 'noDirect' }],
    },
    {
      code: `const v = localStorage.getItem('k');`,
      filename: 'src/renderer/global/runtime/foo.ts',
      errors: [{ messageId: 'noDirect' }],
    },
    {
      code: `window.localStorage.removeItem('k');`,
      filename: 'src/renderer/app/SidebarProvider.tsx',
      errors: [{ messageId: 'noDirect' }],
    },
    {
      code: `globalThis.localStorage.clear();`,
      filename: 'src/renderer/global/service/utils.ts',
      errors: [{ messageId: 'noDirect' }],
    },
    {
      code: `const v = sessionStorage.getItem('k');`,
      filename: 'src/renderer/global/runtime/foo.ts',
      errors: [{ messageId: 'noDirect' }],
    },
  ],
});
