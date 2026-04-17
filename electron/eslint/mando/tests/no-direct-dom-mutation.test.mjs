import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-direct-dom-mutation.mjs';

ruleTester.run('mando/no-direct-dom-mutation', rule, {
  valid: [
    { code: `el.style.color = 'red';` },
    { code: `el.dataset.foo = 'bar';` },
    { code: `el.classList.add('x');` },
  ],
  invalid: [
    { code: `el.textContent = 'hi';`, errors: [{ messageId: 'noMutation' }] },
    { code: `el.innerHTML = '<x />';`, errors: [{ messageId: 'noMutation' }] },
    { code: `el['innerText'] = 'x';`, errors: [{ messageId: 'noMutation' }] },
  ],
});
