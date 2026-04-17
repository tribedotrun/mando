import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/no-ipc-in-ui.mjs';

const uiFile = 'src/renderer/domains/captain/ui/Foo.tsx';
const runtimeFile = 'src/renderer/domains/captain/runtime/hooks.ts';

ruleTester.run('arch/no-ipc-in-ui', rule, {
  valid: [
    { code: `window.mandoAPI.openInFinder('/x');`, filename: runtimeFile },
    { code: `const x = useNativeOpen();`, filename: uiFile },
    { code: `window.addEventListener('click', fn);`, filename: uiFile },
  ],
  invalid: [
    {
      code: `window.mandoAPI.openInFinder('/x');`,
      filename: uiFile,
      errors: [{ messageId: 'noIpc' }],
    },
    {
      code: `window.mandoAPI.selectDirectory();`,
      filename: uiFile,
      errors: [{ messageId: 'noIpc' }],
    },
    {
      code: `const info = await window.mandoAPI.appInfo();`,
      filename: uiFile,
      errors: [{ messageId: 'noIpc' }],
    },
  ],
});
