import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-dom-event-bus.mjs';

ruleTester.run('architecture/no-dom-event-bus', rule, {
  valid: [
    {
      code: `window.addEventListener('keydown', handler);`,
      filename: 'src/renderer/global/runtime/useKeyboardShortcuts.ts',
    },
    {
      code: `document.addEventListener('mousemove', handler);`,
      filename: 'src/renderer/global/runtime/useDevInspector.ts',
    },
    {
      code: `document.addEventListener('keydown', onKey);`,
      filename: 'src/renderer/app/AppHeaderOpenMenu.tsx',
    },
    {
      code: `subscribeViewTaskBrief(handler);`,
      filename: 'src/renderer/domains/captain/runtime/useTaskDetailView.ts',
    },
  ],
  invalid: [
    {
      code: `window.dispatchEvent(new CustomEvent('mando:foo'));`,
      filename: 'src/renderer/app/Sidebar.tsx',
      errors: [{ messageId: 'noDispatchEvent' }, { messageId: 'noCustomEvent' }],
    },
    {
      code: `document.dispatchEvent(new CustomEvent('mando:bar'));`,
      filename: 'src/renderer/global/runtime/foo.ts',
      errors: [{ messageId: 'noDispatchEvent' }, { messageId: 'noCustomEvent' }],
    },
    {
      code: `const e = new CustomEvent('x');`,
      filename: 'src/renderer/global/runtime/foo.ts',
      errors: [{ messageId: 'noCustomEvent' }],
    },
    {
      code: `window.addEventListener('mando:toggle-sidebar', handler);`,
      filename: 'src/renderer/app/routes/AppLayout.tsx',
      errors: [{ messageId: 'noCustomListener' }],
    },
    {
      code: `document.addEventListener('mando:view-task-brief', handler);`,
      filename: 'src/renderer/domains/captain/runtime/useTaskDetailView.ts',
      errors: [{ messageId: 'noCustomListener' }],
    },
  ],
});
