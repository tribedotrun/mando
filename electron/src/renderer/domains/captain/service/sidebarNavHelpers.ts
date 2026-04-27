// Sidebar "New terminal" navigates immediately to the dormant `/wb/new` route.
// `WorkbenchPage` reads `params.workbenchId === 'new'` to render
// `WorkspacePreparing` (full-panel spinner + cancel) on the same tick as the
// click; the actual `createWorktree` call is owned by the page mount effect
// in `useWorkbenchPage.ts`, not the sidebar action.
export function newTerminalNavOptions(project: string) {
  return {
    to: '/wb/$workbenchId' as const,
    params: { workbenchId: 'new' },
    search: { project },
  };
}
