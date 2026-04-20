import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/require-multipart-shadow-body.mjs';

ruleTester.run('architecture/require-multipart-shadow-body', rule, {
  valid: [
    {
      code: `apiMultipartRouteR('postTasksAsk', { id: 1, question: 'hi' });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
    },
    {
      code: `const form = new FormData(); apiMultipartRouteR('postTasksAsk', form, undefined, { id: 1, question: 'hi' });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
    },
    {
      code: `apiMultipartRouteR('postTasksAsk', new FormData(), undefined, { id: 1, question: 'hi' });`,
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
      code: `apiMultipartRouteR('postTasksAsk', new FormData(), { params: { id: 1 } });`,
      filename: 'src/renderer/domains/captain/repo/api.ts',
      errors: [{ messageId: 'requireShadowBody' }],
    },
  ],
});
