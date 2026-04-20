import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-deep-result-import.mjs';

ruleTester.run('mando/no-deep-result-import', rule, {
  valid: [
    {
      filename: '/r/electron/src/renderer/global/providers/http.ts',
      code: `import { Result } from '#result';`,
    },
    // Inside the module, sibling-relative imports are allowed
    {
      filename: '/r/electron/src/shared/result/index.ts',
      code: `import { ok } from './result.ts';`,
    },
    {
      filename: '/r/electron/src/shared/result/__tests__/result.test.ts',
      code: `import { ok } from '../result.ts';`,
    },
  ],
  invalid: [
    {
      filename: '/r/electron/src/renderer/global/providers/http.ts',
      code: `import { _internal } from '#result/result';`,
      errors: [{ messageId: 'deepImport' }],
    },
    {
      filename: '/r/electron/src/renderer/global/foo.ts',
      code: `import { Ok } from '../../shared/result/result';`,
      errors: [{ messageId: 'deepImport' }],
    },
    // #shared/result/* bypasses the barrel — only #result is the allowed entry point
    {
      filename: '/r/electron/src/renderer/global/providers/http.ts',
      code: `import { parseSseMessage } from '#shared/result/sse-parse-handler';`,
      errors: [{ messageId: 'deepImport' }],
    },
    {
      filename: '/r/electron/src/renderer/global/providers/http.ts',
      code: `import { parseSseMessage } from '#shared/result';`,
      errors: [{ messageId: 'deepImport' }],
    },
  ],
});
