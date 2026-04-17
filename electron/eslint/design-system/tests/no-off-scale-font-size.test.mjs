import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-off-scale-font-size.mjs';

ruleTester.run('design-system/no-off-scale-font-size', rule, {
  valid: [
    { code: `const x = <div style={{ fontSize: 14 }} />;` },
    { code: `const x = <div className="text-sm" />;` },
    { code: `const x = <div className="text-[14px]" />;` },
  ],
  invalid: [
    {
      code: `const x = <div style={{ fontSize: 15 }} />;`,
      errors: [{ messageId: 'offScale' }],
    },
    {
      code: `const x = <div className="text-[15px]" />;`,
      errors: [{ messageId: 'offScale' }],
    },
  ],
});
