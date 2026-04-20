import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/main-composition-only.mjs';

ruleTester.run('architecture/main-composition-only', rule, {
  valid: [
    {
      code: `import { app } from 'electron'; const _x = 1; void app.whenReady().then(async () => {});`,
      filename: 'electron/src/main/index.ts',
    },
    // Other main files are not affected by this rule.
    {
      code: `let counter = 0;`,
      filename: 'electron/src/main/global/runtime/foo.ts',
    },
    {
      code: `import fs from 'fs'; fs.readFileSync('/x');`,
      filename: 'electron/src/main/global/runtime/launchd.ts',
    },
  ],
  invalid: [
    {
      code: `let mainWindow = null;`,
      filename: 'electron/src/main/index.ts',
      errors: [{ messageId: 'noTopLevelLet' }],
    },
    {
      code: `let isQuitting = false; let trayAvailable = true;`,
      filename: 'electron/src/main/index.ts',
      errors: [{ messageId: 'noTopLevelLet' }, { messageId: 'noTopLevelLet' }],
    },
    {
      code: `import { execSync } from 'child_process'; execSync('kill -9');`,
      filename: 'electron/src/main/index.ts',
      errors: [{ messageId: 'noTopLevelSyncIo' }],
    },
    {
      code: `import fs from 'fs'; fs.readFileSync('/x', 'utf-8');`,
      filename: 'electron/src/main/index.ts',
      errors: [{ messageId: 'noTopLevelSyncIo' }],
    },
  ],
});
