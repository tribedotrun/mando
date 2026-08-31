import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/require-multipart-shadow-body.mjs';

ruleTester.run('architecture/require-multipart-shadow-body', rule, {
  valid: [
    {
      code: `apiMultipartRouteR('postTasksAdd', { title: 'Task' });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
    },
    {
      code: `const form = new FormData(); apiMultipartRouteR('postTasksAdd', form, undefined, { title: 'Task' });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
    },
    {
      code: `apiMultipartRouteR('postTasksAdd', new FormData(), undefined, { title: 'Task' });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
    },
  ],
  invalid: [
    {
      code: `const form = new FormData(); apiMultipartRouteR('postTasksAdd', form);`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
      errors: [{ messageId: 'requireShadowBody' }],
    },
    {
      code: `apiMultipartRouteR('postTasksAdd', new FormData(), { params: { id: 1 } });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
      errors: [{ messageId: 'requireShadowBody' }],
    },
  ],
});
