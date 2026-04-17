import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-empty-catch.mjs';

ruleTester.run('mando/no-empty-catch', rule, {
  valid: [
    { code: `p.catch((e) => { logger.error(e); });` },
    { code: `p.catch((e) => logger.error(e));` },
  ],
  invalid: [
    { code: `p.catch(() => {});`, errors: [{ messageId: 'emptyCatch' }] },
    { code: `p.catch(() => undefined);`, errors: [{ messageId: 'emptyCatch' }] },
    { code: `p.catch(function () {});`, errors: [{ messageId: 'emptyCatch' }] },
  ],
});
