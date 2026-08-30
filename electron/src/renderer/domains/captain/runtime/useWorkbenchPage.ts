import { useParams, useSearch } from '@tanstack/react-router';
import { useWorkbenchNav } from '#renderer/domains/captain/runtime/useWorkbenchNav';
import { useWorkbenchList } from '#renderer/domains/captain/runtime/hooks';
import { useTaskForWorkbench } from '#renderer/domains/captain/runtime/useTaskForWorkbench';

type WbSearch = {
  tab?: string;
  project?: string;
};

export function useWorkbenchPage() {
  const { workbenchId } = useParams({ strict: false }) as { workbenchId: string };
  const search = useSearch({ strict: false }) as WbSearch;
  const parsedWbId = Number(workbenchId);
  const wbId = Number.isNaN(parsedWbId) ? null : parsedWbId;

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

  const nav = useWorkbenchNav(workbenchId);

  return {
    ids: { workbenchId, wbId },
    search,
    data: { workbench, task, workbenchesLoading, tasksLoading },
    nav,
  };
}
