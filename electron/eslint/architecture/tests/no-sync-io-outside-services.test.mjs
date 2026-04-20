import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-sync-io-outside-services.mjs';

ruleTester.run('architecture/no-sync-io-outside-services', rule, {
  valid: [
    // Service files: anything goes.
    {
      code: `import fs from 'fs'; fs.readFileSync('/x');`,
      filename: 'electron/src/main/global/service/lifecycle.ts',
    },
    // Allowlisted runtime modules.
    {
      code: `import { execSync } from 'child_process'; execSync('launchctl');`,
      filename: 'electron/src/main/global/runtime/launchd.ts',
    },
    {
      code: `import fs from 'fs'; fs.existsSync('/x');`,
      filename: 'electron/src/main/global/runtime/appPackage.ts',
    },
    // Renderer is unaffected.
    {
      code: `import fs from 'fs'; fs.readFileSync('/x');`,
      filename: 'electron/src/renderer/global/runtime/foo.ts',
    },
    // Async fs is fine.
    {
      code: `import fs from 'fs'; await fs.promises.readFile('/x');`,
      filename: 'electron/src/main/global/runtime/foo.ts',
    },
  ],
  invalid: [
    {
      code: `import fs from 'fs'; fs.readFileSync('/x');`,
      filename: 'electron/src/main/global/runtime/foo.ts',
      errors: [{ messageId: 'noSyncIo' }],
    },
    {
      code: `import { execSync } from 'child_process'; execSync('kill');`,
      filename: 'electron/src/main/daemon/runtime/something.ts',
      errors: [{ messageId: 'noSyncIo' }],
    },
    {
      code: `import fs from 'fs'; fs.writeFileSync('/x', 'data');`,
      filename: 'electron/src/main/onboarding/runtime/setup.ts',
      errors: [{ messageId: 'noSyncIo' }],
    },
  ],
});
