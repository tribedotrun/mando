import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/barrel-discipline.mjs';

ruleTester.run('barrel-discipline', rule, {
  valid: [
    { code: `export { useApi } from '#renderer/domains/captain/runtime/useApi';`, filename: 'src/renderer/domains/captain/index.ts' },
    { code: `export { formatDate } from '#renderer/domains/captain/service/format';`, filename: 'src/renderer/domains/captain/index.ts' },
    { code: `export type { TaskItem } from '#renderer/domains/captain/types/task';`, filename: 'src/renderer/domains/captain/index.ts' },
    { code: `export type { TaskItem } from '#renderer/global/types';`, filename: 'src/renderer/domains/captain/index.ts' },
    { code: `export { useTaskList } from '#renderer/domains/captain/repo/queries';`, filename: 'src/renderer/domains/captain/index.ts' },
    { code: `export { x } from '#renderer/domains/captain/ui/Foo';`, filename: 'src/renderer/domains/captain/ui/Foo.tsx' },
  ],
  invalid: [
    { code: `export { CaptainView } from '#renderer/domains/captain/ui/CaptainView';`, filename: 'src/renderer/domains/captain/index.ts', errors: [{ messageId: 'badExport' }] },
    { code: `export { http } from '#renderer/global/providers/http';`, filename: 'src/renderer/domains/captain/index.ts', errors: [{ messageId: 'badExport' }] },
    { code: `export { x } from '#renderer/global/config/foo';`, filename: 'src/renderer/domains/captain/index.ts', errors: [{ messageId: 'badExport' }] },
  ],
});
