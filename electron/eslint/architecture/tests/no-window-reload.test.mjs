import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-window-reload.mjs';

ruleTester.run('architecture/no-window-reload', rule, {
  valid: [
    {
      code: `router.invalidate();`,
      filename: 'src/renderer/global/runtime/useNativeActions.ts',
    },
    {
      code: `window.location.href = '/next';`,
      filename: 'src/renderer/global/runtime/useNativeActions.ts',
    },
  ],
  invalid: [
    {
      code: `window.location.reload();`,
      filename: 'src/renderer/global/runtime/useNativeActions.ts',
      errors: [{ messageId: 'noReload' }],
    },
    {
      code: `location.reload();`,
      filename: 'src/renderer/domains/onboarding/ui/OnboardingWizard.tsx',
      errors: [{ messageId: 'noReload' }],
    },
    {
      code: `document.location.reload();`,
      filename: 'src/renderer/domains/onboarding/ui/OnboardingWizard.tsx',
      errors: [{ messageId: 'noReload' }],
    },
  ],
});
