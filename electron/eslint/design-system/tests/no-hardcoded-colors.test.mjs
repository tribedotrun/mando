import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-hardcoded-colors.mjs';

ruleTester.run('design-system/no-hardcoded-colors', rule, {
  valid: [
    { code: `const x = <div style={{ color: 'var(--text)' }} />;` },
    { code: `const x = <div style={{ color: 'transparent' }} />;` },
    { code: `const x = <div style={{ boxShadow: '0 0 0 #fff' }} />;` },
    { code: `const x = <div className="bg-foreground text-muted" />;` },
  ],
  invalid: [
    {
      code: `const x = <div style={{ color: '#ffffff' }} />;`,
      errors: [{ messageId: 'hex' }],
    },
    {
      code: `const x = <div style={{ background: 'rgba(0,0,0,0.5)' }} />;`,
      errors: [{ messageId: 'rgb' }],
    },
    {
      code: `const x = <div style={{ color: 'red' }} />;`,
      errors: [{ messageId: 'named' }],
    },
    {
      code: `const x = <div className="bg-[#fff] p-2" />;`,
      errors: [{ messageId: 'hex' }],
    },
    {
      code: `const x = <div className="text-[rgb(0,0,0)]" />;`,
      errors: [{ messageId: 'rgb' }],
    },
  ],
});
