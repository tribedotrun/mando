import rule from '../rules/toast-only-in-ui.mjs';
import { ruleTester } from '../../test-setup.mjs';

const toastImport = `import { toast } from '#renderer/global/runtime/useFeedback';`;

ruleTester.run('architecture/toast-only-in-ui', rule, {
  valid: [
    {
      filename: 'src/renderer/domains/captain/runtime/useTaskActions.ts',
      code: toastImport,
    },
    {
      filename: 'src/renderer/global/runtime/useNativeActions.ts',
      code: toastImport,
    },
    {
      filename: 'src/renderer/domains/captain/ui/TaskActions.tsx',
      code: toastImport,
    },
  ],
  invalid: [
    {
      filename: 'src/renderer/domains/captain/repo/mutations.ts',
      code: toastImport,
      errors: [{ messageId: 'noToastOutsideUi' }],
    },
    {
      filename: 'src/renderer/global/runtime/sseEvents.ts',
      code: toastImport,
      errors: [{ messageId: 'noToastOutsideUi' }],
    },
  ],
});
