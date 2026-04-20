import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-direct-fetch.mjs';

ruleTester.run('mando/no-direct-fetch', rule, {
  valid: [
    {
      filename: '/r/electron/src/renderer/global/providers/http.ts',
      code: `await fetch(url);`,
    },
    {
      filename: '/r/electron/src/main/global/runtime/lifecycle.ts',
      code: `await fetch(url);`,
    },
    {
      filename: '/r/electron/src/renderer/domains/captain/repo/api.ts',
      code: `await apiGetRoute('getTasks');`,
    },
  ],
  invalid: [
    {
      filename: '/r/electron/src/renderer/domains/captain/repo/api.ts',
      code: `await fetch('/api/tasks');`,
      errors: [{ messageId: 'direct' }],
    },
    {
      filename: '/r/electron/src/main/some/other/file.ts',
      code: `await fetch(url);`,
      errors: [{ messageId: 'direct' }],
    },
  ],
});
