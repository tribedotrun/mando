import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-react-query-creators-outside-repo.mjs';

ruleTester.run('architecture/no-react-query-creators-outside-repo', rule, {
  valid: [
    {
      code: `import { useQuery, useMutation } from '@tanstack/react-query';`,
      filename: 'src/renderer/domains/captain/repo/queries.ts',
    },
    {
      code: `import { useQueryClient } from '@tanstack/react-query';`,
      filename: 'src/renderer/domains/captain/runtime/useTaskActions.ts',
    },
    {
      code: `import { QueryClientProvider } from '@tanstack/react-query';`,
      filename: 'src/renderer/app/DataProvider.tsx',
    },
  ],
  invalid: [
    {
      code: `import { useQuery } from '@tanstack/react-query';`,
      filename: 'src/renderer/global/runtime/useConfig.ts',
      errors: [{ messageId: 'noCreator' }],
    },
    {
      code: `import { useMutation } from '@tanstack/react-query';`,
      filename: 'src/renderer/domains/settings/runtime/useUpdateLifecycle.ts',
      errors: [{ messageId: 'noCreator' }],
    },
    {
      code: `import { useInfiniteQuery as createInfinite } from '@tanstack/react-query';`,
      filename: 'src/renderer/domains/sessions/runtime/useTranscript.ts',
      errors: [{ messageId: 'noCreator' }],
    },
  ],
});
