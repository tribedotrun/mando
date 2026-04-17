import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/no-business-logic-in-ui.mjs';

const uiFile = 'src/renderer/domains/captain/ui/Foo.tsx';
const serviceFile = 'src/renderer/global/service/format.ts';

ruleTester.run('arch/no-business-logic-in-ui', rule, {
  valid: [
    { code: `Math.floor(1.5);`, filename: serviceFile },
    { code: `parseInt('1');`, filename: serviceFile },
    { code: `const x = formatPrice(value);`, filename: uiFile },
  ],
  invalid: [
    {
      code: `const x = Math.floor(1.5);`,
      filename: uiFile,
      errors: [{ messageId: 'noLogic' }],
    },
    {
      code: `const x = parseInt('1');`,
      filename: uiFile,
      errors: [{ messageId: 'noLogic' }],
    },
    {
      code: `const x = (1.5).toFixed(2);`,
      filename: uiFile,
      errors: [{ messageId: 'noLogic' }],
    },
    {
      code: `const x = Number('1');`,
      filename: uiFile,
      errors: [{ messageId: 'noLogic' }],
    },
  ],
});
