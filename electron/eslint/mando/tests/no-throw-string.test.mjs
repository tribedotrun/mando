import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-throw-string.mjs';

ruleTester.run('mando/no-throw-string', rule, {
  valid: [
    { code: `throw new Error('boom');` },
    { code: `throw new HttpError('boom', 500);` },
    { code: `throw err;` },
  ],
  invalid: [
    { code: `throw 'boom';`, errors: [{ messageId: 'throwString' }] },
    { code: `throw \`boom \${x}\`;`, errors: [{ messageId: 'throwString' }] },
  ],
});
