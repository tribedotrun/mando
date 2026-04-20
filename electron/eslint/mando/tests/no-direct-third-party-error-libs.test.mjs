import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-direct-third-party-error-libs.mjs';

ruleTester.run('mando/no-direct-third-party-error-libs', rule, {
  valid: [
    { code: `import { Result, ok, err } from '#result';` },
    { code: `import { z } from 'zod';` },
    { code: `import x from 'react';` },
  ],
  invalid: [
    {
      code: `import { Result } from 'neverthrow';`,
      errors: [{ messageId: 'banned' }],
    },
    {
      code: `import { ok } from 'oxide.ts';`,
      errors: [{ messageId: 'banned' }],
    },
    {
      code: `import { err } from 'ts-results';`,
      errors: [{ messageId: 'banned' }],
    },
    {
      code: `import { Effect } from 'effect';`,
      errors: [{ messageId: 'banned' }],
    },
  ],
});
