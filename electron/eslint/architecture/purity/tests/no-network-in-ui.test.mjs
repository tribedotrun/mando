import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/no-network-in-ui.mjs';

const uiFile = 'src/renderer/domains/captain/ui/Foo.tsx';
const repoFile = 'src/renderer/domains/captain/repo/api.ts';
const runtimeFile = 'src/renderer/domains/captain/runtime/hooks.ts';

ruleTester.run('arch/no-network-in-ui', rule, {
  valid: [
    { code: `fetch('/x');`, filename: repoFile },
    { code: `const x = useThing();`, filename: uiFile },
    { code: `import { apiGet } from '#renderer/global/providers/http';`, filename: runtimeFile },
    { code: `import { useQuery } from '@tanstack/react-query';`, filename: runtimeFile },
    { code: `import { useSomeMutation } from '#renderer/domains/captain/runtime/hooks';`, filename: uiFile },
  ],
  invalid: [
    {
      code: `fetch('/x');`,
      filename: uiFile,
      errors: [{ messageId: 'noFetch' }],
    },
    {
      code: `window.fetch('/x');`,
      filename: uiFile,
      errors: [{ messageId: 'noFetch' }],
    },
    {
      code: `import { apiGet } from '#renderer/global/providers/http';`,
      filename: uiFile,
      errors: [{ messageId: 'noHttpProvider' }],
    },
    {
      code: `import { apiPost, apiDel } from '#renderer/domains/settings/runtime/useApi';`,
      filename: uiFile,
      errors: [{ messageId: 'noHttpFn' }],
    },
    {
      code: `import { apiGet } from '#renderer/domains/settings/runtime/useApi';`,
      filename: uiFile,
      errors: [{ messageId: 'noHttpFn' }],
    },
    {
      code: `import { useQuery } from '@tanstack/react-query';`,
      filename: uiFile,
      errors: [{ messageId: 'noReactQuery' }],
    },
    {
      code: `import { useMutation } from '@tanstack/react-query';`,
      filename: uiFile,
      errors: [{ messageId: 'noReactQuery' }],
    },
  ],
});
