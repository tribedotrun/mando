import { useNavigate, useParams, useSearch } from '@tanstack/react-router';
import { useCallback, useMemo, useRef } from 'react';
import { useWorkbenchNav } from '#renderer/domains/captain/runtime/useWorkbenchNav';
import { useWorkbenchList } from '#renderer/domains/captain/runtime/hooks';
import { useTaskForWorkbench } from '#renderer/domains/captain/runtime/useTaskForWorkbench';
import { useWorktreeTerminal } from '#renderer/domains/captain/terminal/runtime/useWorktreeTerminal';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useConfig } from '#renderer/global/repo/queries';
import { resolveProjectPath } from '#renderer/domains/captain/service/projectHelpers';

type WbSearch = { tab?: string; resume?: string; name?: string; project?: string };

export function useWorkbenchPage() {
  const navigate = useNavigate();
  const { workbenchId } = useParams({ strict: false }) as { workbenchId: string };
  const search = useSearch({ strict: false }) as WbSearch;
  const isNewWorkbench = workbenchId === 'new';
  const wbId = isNewWorkbench ? null : Number(workbenchId);

  // For "new" workbench creation flow
  const { terminalPage, openNewTerminal, cancelPreparing } = useWorktreeTerminal();
  const creationStarted = useRef(false);

  useMountEffect(() => {
    if (isNewWorkbench && !search.project) {
      void navigate({ to: '/', replace: true });
      return;
    }
    if (isNewWorkbench && search.project && !creationStarted.current) {
      creationStarted.current = true;
      void openNewTerminal(search.project, (_cwd, result) => {
        if (result?.workbenchId) {
          void navigate({
            to: '/wb/$workbenchId',
            params: { workbenchId: String(result.workbenchId) },
            search: { tab: 'terminal' },
            replace: true,
          });
        }
      });
    }
  });

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

  // Project root for the workbench's project. Clarifier sessions store
  // cwd = project root rather than worktree, so the terminal panel widens
  // its filter to include this path; resumed clarifier terminals would
  // otherwise fall outside the worktree-scoped session list.
  const { data: config } = useConfig();
  const projectRoot = useMemo(
    () => resolveProjectPath(config?.captain?.projects, workbench?.project),
    [config?.captain?.projects, workbench?.project],
  );
  const extraTerminalCwds = useMemo(
    () => (projectRoot && projectRoot !== workbench?.worktree ? [projectRoot] : []),
    [projectRoot, workbench?.worktree],
  );

  const handleCancelNew = useCallback(() => {
    cancelPreparing();
    void navigate({ to: '/', replace: true });
  }, [cancelPreparing, navigate]);

  const nav = useWorkbenchNav(workbenchId, search);

  return {
    ids: { workbenchId, wbId, isNewWorkbench },
    search,
    terminal: { page: terminalPage, extraCwds: extraTerminalCwds },
    data: { workbench, task, workbenchesLoading, tasksLoading },
    actions: { handleCancelNew },
    nav,
  };
}
