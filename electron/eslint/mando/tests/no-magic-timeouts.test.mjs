import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-magic-timeouts.mjs';

ruleTester.run('mando/no-magic-timeouts', rule, {
  valid: [
    { code: `setTimeout(fn, DELAY_MS);` },
    { code: `setTimeout(fn, 0);` }, // default-allowed for setTimeout
    { code: `setInterval(fn, INTERVAL);` },
    { code: `window.setTimeout(fn, 0);` },
    // user-supplied allow extends both timers
    { code: `setInterval(fn, 16);`, options: [{ allow: [16] }] },
  ],
  invalid: [
    { code: `setTimeout(fn, 100);`, errors: [{ messageId: 'magic' }] },
    { code: `setInterval(fn, 5000);`, errors: [{ messageId: 'magic' }] },
    { code: `window.setTimeout(fn, 250);`, errors: [{ messageId: 'magic' }] },
    // setInterval(fn, 0) is intentionally NOT default-allowed: it's a busy loop.
    { code: `setInterval(fn, 0);`, errors: [{ messageId: 'magic' }] },
    { code: `window.setInterval(fn, 0);`, errors: [{ messageId: 'magic' }] },
  ],
});
