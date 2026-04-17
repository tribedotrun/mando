import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-off-grid-radius.mjs';

ruleTester.run('design-system/no-off-grid-radius', rule, {
  valid: [
    { code: `const x = <div style={{ borderRadius: 8 }} />;` },
    { code: `const x = <div style={{ borderRadius: '50%' }} />;` },
    { code: `const x = <div style={{ borderRadius: 'var(--r)' }} />;` },
    { code: `const x = <div className="rounded-md" />;` },
    { code: `const x = <div className="rounded-[8px]" />;` },
  ],
  invalid: [
    {
      code: `const x = <div style={{ borderRadius: 7 }} />;`,
      errors: [{ messageId: 'offGrid' }],
    },
    {
      code: `const x = <div className="rounded-[7px]" />;`,
      errors: [{ messageId: 'offGrid' }],
    },
    {
      code: `const x = <div className="rounded-tl-[7px]" />;`,
      errors: [{ messageId: 'offGrid' }],
    },
  ],
});
