import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-raw-contract-ipc-send.mjs';

ruleTester.run('architecture/no-raw-contract-ipc-send', rule, {
  valid: [
    {
      code: `sendChannel(win.webContents, 'shortcut', 'add-task');`,
      filename: 'src/main/index.ts',
    },
    {
      code: `target.send(channel, payload);`,
      filename: 'src/main/global/runtime/ipcSecurity.ts',
    },
    {
      code: `win.webContents.send('non-contract-channel', { ok: true });`,
      filename: 'src/main/global/runtime/foo.ts',
    },
  ],
  invalid: [
    {
      code: `win.webContents.send('shortcut', 'add-task');`,
      filename: 'src/main/index.ts',
      errors: [{ messageId: 'useSendChannel' }],
    },
    {
      code: `event.sender.send('setup-progress', 'saving');`,
      filename: 'src/main/onboarding/repo/config.ts',
      errors: [{ messageId: 'useSendChannel' }],
    },
    {
      code: `window.webContents.send('notification-click', { notificationId: 'n1' });`,
      filename: 'src/main/shell/runtime/notifications.ts',
      errors: [{ messageId: 'useSendChannel' }],
    },
  ],
});
