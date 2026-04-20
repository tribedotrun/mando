import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/app-purity.mjs';

ruleTester.run('app-purity', rule, {
  valid: [
    { code: `import { x } from '#renderer/global/runtime/useSseSync';`, filename: 'src/renderer/app/DataProvider.tsx' },
    { code: `import { x } from '#renderer/global/ui/button';`, filename: 'src/renderer/app/routes/Foo.tsx' },
    { code: `import { x } from '#renderer/domains/captain/ui/CaptainView';`, filename: 'src/renderer/app/routes/Foo.tsx' },
    { code: `import { x } from '#renderer/global/repo/queries';`, filename: 'src/renderer/app/routes/Foo.tsx' },
    { code: `import { x } from '#renderer/global/service/utils';`, filename: 'src/renderer/app/Sidebar.tsx' },
    { code: `import { QueryClientProvider } from '@tanstack/react-query';`, filename: 'src/renderer/app/DataProvider.tsx' },
    { code: `import { useQueryClient } from '@tanstack/react-query';`, filename: 'src/renderer/app/SidebarProvider.tsx' },
    { code: `import { queryClient } from '#renderer/global/providers/queryClient';`, filename: 'src/renderer/app/DataProvider.tsx' },
    { code: `import { subscribeObsDegraded } from '#renderer/global/providers/obsHealth';`, filename: 'src/renderer/app/DataProvider.tsx' },
    { code: `import { x } from '#renderer/domains/captain/repo/api';`, filename: 'src/renderer/domains/captain/runtime/useApi.ts' },
    { code: `import { x } from '#renderer/domains/captain';`, filename: 'src/renderer/app/Sidebar.tsx' },
    { code: `const x = 1;`, filename: 'src/renderer/app/Sidebar.tsx' },
    { code: `function useLocalThing() {}\nexport function Sidebar() { useLocalThing(); return null; }`, filename: 'src/renderer/app/Sidebar.tsx' },
    { code: `export function DataProvider() { return null; }`, filename: 'src/renderer/app/DataProvider.tsx' },
  ],
  invalid: [
    { code: `import { x } from '#renderer/domains/captain/repo/api';`, filename: 'src/renderer/app/routes/Foo.tsx', errors: [{ messageId: 'impure' }] },
    { code: `import { x } from '#renderer/domains/captain/service/format';`, filename: 'src/renderer/app/Sidebar.tsx', errors: [{ messageId: 'impure' }] },
    { code: `import { x } from '#renderer/domains/scout/runtime/useScout';`, filename: 'src/renderer/app/DataProvider.tsx', errors: [{ messageId: 'impure' }] },
    { code: `import { apiPost } from '#renderer/global/providers/http';`, filename: 'src/renderer/app/SidebarProvider.tsx', errors: [{ messageId: 'noHttpProvider' }] },
    { code: `import { useQuery } from '@tanstack/react-query';`, filename: 'src/renderer/app/AppHeader.tsx', errors: [{ messageId: 'noReactQuery' }] },
    { code: `window.mandoAPI.openInFinder('/x');`, filename: 'src/renderer/app/SidebarProvider.tsx', errors: [{ messageId: 'noIpc' }] },
    { code: `export function useSidebarData() { return null; }`, filename: 'src/renderer/app/useSidebarData.ts', errors: [{ messageId: 'noExportedHooks' }] },
    { code: `export const useWorkbenchNav = () => null;`, filename: 'src/renderer/app/useWorkbenchNav.ts', errors: [{ messageId: 'noExportedHooks' }] },
    { code: `export { useWorkbenchPage } from '#renderer/global/runtime/useWorkbenchPage';`, filename: 'src/renderer/app/index.ts', errors: [{ messageId: 'noExportedHooks' }] },
    { code: `export function SidebarProvider() { return null; }`, filename: 'src/renderer/app/SidebarProvider.tsx', errors: [{ messageId: 'noCustomProvider' }] },
    {
      code: `import { create } from 'zustand';\nexport const useUIStore = create(() => ({}));`,
      filename: 'src/renderer/app/uiStore.ts',
      errors: [{ messageId: 'noZustand' }, { messageId: 'noExportedHooks' }],
    },
  ],
});
