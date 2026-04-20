import { useNavigate, useParams, useSearch } from '@tanstack/react-router';
import { useCallback, useRef } from 'react';
import { useWorkbenchNav } from '#renderer/domains/captain/runtime/useWorkbenchNav';
import { useTaskList, useWorkbenchList } from '#renderer/domains/captain/runtime/hooks';
import { useWorktreeTerminal } from '#renderer/domains/captain/terminal/runtime/useWorktreeTerminal';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';

type WbSearch = { tab?: string; resume?: string; name?: string; project?: string };

export function useWorkbenchPage() {
  const navigate = useNavigate();
  const { workbenchId } = useParams({ strict: false }) as { workbenchId: string };
  const search = useSearch({ strict: false }) as WbSearch;
  const isNewWorkbench = workbenchId === 'new';
  const wbId = isNewWorkbench ? null : Number(workbenchId);

  // Key the TerminalPage so it remounts when a resume is requested.
  // Ref persists the key after the resume param is consumed from the URL.
  const terminalKeyRef = useRef(search.resume ?? '');
  if (search.resume && search.resume !== terminalKeyRef.current) {
    terminalKeyRef.current = search.resume;
  }
  const terminalKey = terminalKeyRef.current;

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
  const { data: taskData, isLoading: tasksLoading } = useTaskList();
  const workbench = activeMatch ?? (wbId ? (allWbs.find((w) => w.id === wbId) ?? null) : null);
  const task = taskData?.items.find((t) => t.workbench_id === wbId) ?? null;

  const handleCancelNew = useCallback(() => {
    cancelPreparing();
    void navigate({ to: '/', replace: true });
  }, [cancelPreparing, navigate]);

  const nav = useWorkbenchNav(workbenchId, search);

  return {
    workbenchId,
    wbId,
    isNewWorkbench,
    search,
    terminalKey,
    terminalPage,
    workbench,
    task,
    workbenchesLoading,
    tasksLoading,
    handleCancelNew,
    ...nav,
  };
}
