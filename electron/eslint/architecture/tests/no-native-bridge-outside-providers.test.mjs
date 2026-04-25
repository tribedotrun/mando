import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-native-bridge-outside-providers.mjs';

ruleTester.run('architecture/no-native-bridge-outside-providers', rule, {
  valid: [
    {
      code: `window.mandoAPI.openInFinder('/tmp');`,
      filename: 'src/renderer/global/providers/native/shell.ts',
    },
    {
      code: `const x = useNativeActions();`,
      filename: 'src/renderer/domains/settings/runtime/useSettings.ts',
    },
  ],
  invalid: [
    {
      code: `window.mandoAPI.openInFinder('/tmp');`,
      filename: 'src/renderer/domains/onboarding/providers/ipcBridge.ts',
      errors: [{ messageId: 'noDirectBridge' }],
    },
    {
      code: `window.mandoAPI.openInFinder('/tmp');`,
      filename: 'src/renderer/global/runtime/useNativeActions.ts',
      errors: [{ messageId: 'noDirectBridge' }],
    },
    {
      code: `window.mandoAPI?.updates?.checkForUpdates?.();`,
      filename: 'src/renderer/domains/settings/runtime/useUpdateLifecycle.ts',
      errors: [{ messageId: 'noDirectBridge' }],
    },
  ],
});
