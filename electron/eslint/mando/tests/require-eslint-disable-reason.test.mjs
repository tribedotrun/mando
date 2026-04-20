import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/require-eslint-disable-reason.mjs';

ruleTester.run('mando/require-eslint-disable-reason', rule, {
  valid: [
    {
      code: `// eslint-disable-next-line no-unused-vars -- reason: lib bug, see #1234
        const x = 1;`,
    },
    { code: `/* eslint-disable no-unused-vars -- reason: legacy code */ const x = 1;` },
    { code: `const x = 1; // not a disable comment` },
  ],
  invalid: [
    {
      code: `// eslint-disable-next-line no-unused-vars
        const x = 1;`,
      errors: [{ messageId: 'missing' }],
    },
    {
      code: `const x = 1; // eslint-disable-line no-unused-vars`,
      errors: [{ messageId: 'missing' }],
    },
    {
      code: `/* eslint-disable no-unused-vars */ const x = 1;`,
      errors: [{ messageId: 'missing' }],
    },
  ],
});
