import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/zustand-fine-grained.mjs';

ruleTester.run('architecture/zustand-fine-grained', rule, {
  valid: [
    {
      code: `import { useUIStore } from '#renderer/global/runtime/useUIStore'; const a = useUIStore(s => s.a); useUIStore.getState().open();`,
      filename: 'src/renderer/app/Root.tsx',
    },
    {
      code: `import { useUIStore } from '#renderer/global/runtime/useUIStore'; const pair = useUIStore(s => ({ a: s.a, b: s.b, c: s.c }));`,
      filename: 'src/renderer/app/Root.tsx',
    },
  ],
  invalid: [
    {
      code: `import { useUIStore } from '#renderer/global/runtime/useUIStore'; const { a,b,c,d } = useUIStore();`,
      filename: 'src/renderer/app/Root.tsx',
      errors: [{ messageId: 'bare' }],
    },
    {
      code: `import { useUIStore } from '#renderer/global/runtime/useUIStore'; const bag = useUIStore(s => ({ a: s.a, b: s.b, c: s.c, d: s.d }));`,
      filename: 'src/renderer/app/Root.tsx',
      errors: [{ messageId: 'selector' }],
    },
  ],
});
