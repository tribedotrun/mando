import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/query-key-factory.mjs';

ruleTester.run('architecture/query-key-factory', rule, {
  valid: [
    {
      code: `useQuery({ queryKey: queryKeys.tasks.list(), queryFn });`,
      filename: 'src/renderer/domains/captain/repo/queries.ts',
    },
    {
      code: `useQuery({ queryKey: queryKeys.highlighter.code(lang, code), queryFn });`,
      filename: 'src/renderer/global/runtime/useHighlightQuery.ts',
    },
    // Spreading + suffix is fine; first element is not an inline string literal at the top level.
    {
      code: `useQuery({ queryKey: [...queryKeys.tasks.list(), 'with-archived'], queryFn });`,
      filename: 'src/renderer/domains/captain/repo/queries.ts',
    },
    // The factory file itself is exempt.
    {
      code: `export const queryKeys = { tasks: { all: ['tasks'] as const } };`,
      filename: 'src/renderer/global/repo/queryKeys.ts',
    },
  ],
  invalid: [
    {
      code: `useQuery({ queryKey: ['shiki-highlight', lang, code], queryFn });`,
      filename: 'src/renderer/global/runtime/useHighlightQuery.ts',
      errors: [{ messageId: 'inlineKey' }],
    },
    {
      code: `useQuery({ queryKey: ['settings', 'about', 'appVersion'], queryFn });`,
      filename: 'src/renderer/domains/settings/runtime/useAppVersion.ts',
      errors: [{ messageId: 'inlineKey' }],
    },
    {
      code: `useMutation({ mutationKey: ['tasks', 'create'], mutationFn });`,
      filename: 'src/renderer/domains/captain/repo/mutations.ts',
      errors: [{ messageId: 'inlineKey' }],
    },
    // Dynamic template literal as the namespace element also bypasses the factory.
    {
      code: 'useQuery({ queryKey: [`tasks-${id}`, id], queryFn });',
      filename: 'src/renderer/domains/captain/repo/queries.ts',
      errors: [{ messageId: 'inlineKey' }],
    },
  ],
});
