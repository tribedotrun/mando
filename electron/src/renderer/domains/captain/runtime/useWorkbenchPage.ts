import { useNavigate, useParams, useSearch } from '@tanstack/react-router';
import { useCallback, useRef } from 'react';
import { useWorkbenchNav } from '#renderer/domains/captain/runtime/useWorkbenchNav';
import { useWorkbenchList } from '#renderer/domains/captain/runtime/hooks';
import { useTaskForWorkbench } from '#renderer/domains/captain/runtime/useTaskForWorkbench';
import { useWorktreeTerminal } from '#renderer/domains/captain/terminal/runtime/useWorktreeTerminal';

type WbSearch = { tab?: string; resume?: string; name?: string; project?: string };

export function useWorkbenchPage() {
  const navigate = useNavigate();
  const { workbenchId } = useParams({ strict: false }) as { workbenchId: string };
  const search = useSearch({ strict: false }) as WbSearch;
  const isNewWorkbench = workbenchId === 'new';
  const wbId = isNewWorkbench ? null : Number(workbenchId);

  // For "new" workbench creation flow
  const { openNewTerminal, cancelPreparing } = useWorktreeTerminal();
  const creationStarted = useRef(false);
  const redirectStarted = useRef(false);

  // TanStack Router reuses WorkbenchPage when only `params.workbenchId` or
  // `search.project` changes between values of the same route. The previous
  // mount-effect therefore did NOT re-run when a user already on a workbench
  // clicked "+ New terminal" (workbenchId went '<id>' -> 'new') or clicked
  // "+ New terminal" for a different project mid-creation (workbenchId stays
  // 'new' but search.project changes), leaving them on the dead fallback.
  // Mirror the prevWbRef pattern in useWorkbenchNav.ts so the new-workbench
  // creation branch re-fires on every identity change, not just first mount.
  const prevWbRef = useRef(workbenchId);
  const prevProjectRef = useRef(search.project);
  if (prevWbRef.current !== workbenchId || prevProjectRef.current !== search.project) {
    prevWbRef.current = workbenchId;
    prevProjectRef.current = search.project;
    creationStarted.current = false;
    redirectStarted.current = false;
    if (isNewWorkbench) {
      // Drop any stale terminalPage / in-flight creation from a previous
      // /wb/new flow so the next openNewTerminal call below starts cleanly
      // (cancelPreparing also resets `creatingRef` so it isn't silently
      // blocked behind the old in-flight createWorktree).
      cancelPreparing();
    }
  }

  if (isNewWorkbench && !search.project && !redirectStarted.current) {
    redirectStarted.current = true;
    queueMicrotask(() => void navigate({ to: '/', replace: true }));
  } else if (isNewWorkbench && search.project && !creationStarted.current) {
    creationStarted.current = true;
    const project = search.project;
    queueMicrotask(() => {
      void openNewTerminal(
        project,
        (_cwd, result) => {
          if (result?.workbenchId) {
            void navigate({
              to: '/wb/$workbenchId',
              params: { workbenchId: String(result.workbenchId) },
              search: { tab: 'terminal' },
              replace: true,
            });
          }
        },
        () => {
          // createWorktree rejected. The toast and setTerminalPage(null)
          // already happened inside openNewTerminal; navigate home so the
          // user is never stranded on /wb/new without recovery.
          void navigate({ to: '/', replace: true });
        },
      );
    });
  }

  // Use active list (Tier 1, zero refetch) as primary source.
  // Only fetch 'all' when the workbench isn't in the active cache (archived).
  const { data: activeWbs = [], isLoading: activeLoading } = useWorkbenchList();
  const activeMatch = wbId ? (activeWbs.find((w) => w.id === wbId) ?? null) : null;
  const { data: allWbs = [], isLoading: allLoading } = useWorkbenchList(
    wbId && !activeMatch ? 'all' : undefined,
  );
  const workbenchesLoading = activeLoading || (!activeMatch && allLoading);
  const workbench = activeMatch ?? (wbId ? (allWbs.find((w) => w.id === wbId) ?? null) : null);
  const { task, isLoading: tasksLoading } = useTaskForWorkbench(wbId, workbench);

  const handleCancelNew = useCallback(() => {
    cancelPreparing();
    void navigate({ to: '/', replace: true });
  }, [cancelPreparing, navigate]);

  const nav = useWorkbenchNav(workbenchId, search);

  return {
    ids: { workbenchId, wbId, isNewWorkbench },
    search,
    data: { workbench, task, workbenchesLoading, tasksLoading },
    actions: { handleCancelNew },
    nav,
  };
}
