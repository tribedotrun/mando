import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-self-import.mjs';

ruleTester.run('mando/no-self-import', rule, {
  valid: [
    { code: `import x from './other';`, filename: '/abs/foo.ts' },
    { code: `import x from '#renderer/api';`, filename: '/abs/foo.ts' },
  ],
  invalid: [
    {
      code: `import x from './foo';`,
      filename: '/abs/foo.ts',
      errors: [{ messageId: 'selfImport' }],
    },
    {
      code: `import x from './foo.ts';`,
      filename: '/abs/foo.ts',
      errors: [{ messageId: 'selfImport' }],
    },
  ],
});
