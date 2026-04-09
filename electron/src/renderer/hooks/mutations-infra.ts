import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { addScoutUrl, bulkUpdateScout, bulkDeleteScout, apiPost, apiPut } from '#renderer/api';
import {
  createTerminal,
  deleteTerminal,
  archiveWorkbench,
  pinWorkbench,
  type CreateTerminalParams,
  type TerminalSessionInfo,
  type WorkbenchItem,
} from '#renderer/api-terminal';
import type { ScoutResponse, MandoConfig } from '#renderer/types';
import { queryKeys } from '#renderer/queryKeys';

// ---------------------------------------------------------------------------
// useScoutAdd
// ---------------------------------------------------------------------------

export function useScoutAdd() {
  return useMutation({
    mutationFn: async (vars: { url: string; title?: string }) => addScoutUrl(vars.url, vars.title),
    onError: () => {
      toast.error('Failed to add scout item');
    },
    // SSE handles cache update
  });
}

// ---------------------------------------------------------------------------
// useScoutBulkUpdate
// ---------------------------------------------------------------------------

export function useScoutBulkUpdate() {
  return useMutation({
    mutationFn: async (vars: { ids: number[]; updates: { status: string } }) =>
      bulkUpdateScout(vars.ids, vars.updates),
    onError: () => {
      toast.error('Bulk update failed');
    },
    // SSE handles cache update
  });
}

// ---------------------------------------------------------------------------
// useScoutBulkDelete
// ---------------------------------------------------------------------------

export function useScoutBulkDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { ids: number[] }) => bulkDeleteScout(vars.ids),
    onMutate: async (vars) => {
      // Cancel all scout list queries regardless of params
      await qc.cancelQueries({ queryKey: queryKeys.scout.all });
      const idSet = new Set(vars.ids);
      // Optimistically remove from all cached scout list pages
      qc.setQueriesData<ScoutResponse>({ queryKey: queryKeys.scout.all }, (old) => {
        if (!old?.items) return old;
        const items = old.items.filter((item) => !idSet.has(item.id));
        return {
          ...old,
          items,
          count: items.length,
          total: old.total - (old.items.length - items.length),
        };
      });
      return {}; // No single prev to restore; SSE will reconcile
    },
    onError: () => {
      // Refetch to restore correct state
      void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
      toast.error('Bulk delete failed');
    },
  });
}

// ---------------------------------------------------------------------------
// useTerminalCreate
// ---------------------------------------------------------------------------

export function useTerminalCreate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (params: CreateTerminalParams) => createTerminal(params),
    onSuccess: (newSession) => {
      qc.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old ? [...old, newSession] : [newSession],
      );
    },
    onError: () => {
      toast.error('Failed to create terminal');
    },
  });
}

// ---------------------------------------------------------------------------
// useTerminalDelete
// ---------------------------------------------------------------------------

export function useTerminalDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: string }) => deleteTerminal(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.terminals.list() });
      const prev = qc.getQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list());
      qc.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old?.filter((t) => t.id !== vars.id),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.terminals.list(), context.prev);
      toast.error('Failed to delete terminal');
    },
  });
}

// ---------------------------------------------------------------------------
// useConfigSave
// ---------------------------------------------------------------------------

export function useConfigSave() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (config: MandoConfig) => apiPut<{ ok: boolean }>('/api/config', config),
    onError: () => {
      toast.error('Failed to save config');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useProjectAdd
// ---------------------------------------------------------------------------

export function useProjectAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { path: string }) =>
      apiPost<{ ok: boolean; name: string; path: string; githubRepo: string }>('/api/projects', {
        path: vars.path,
      }),
    onError: () => {
      toast.error('Failed to add project');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useProjectRemove
// ---------------------------------------------------------------------------

export function useProjectRemove() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { pathKey: string }) => {
      const current = qc.getQueryData<MandoConfig>(queryKeys.config.current());
      if (!current) throw new Error('Config not loaded');

      const projects = { ...(current.captain?.projects ?? {}) };
      delete projects[vars.pathKey];
      const updated: MandoConfig = {
        ...current,
        captain: { ...current.captain, projects },
      };

      return apiPut<{ ok: boolean }>('/api/config', updated);
    },
    onError: () => {
      toast.error('Failed to remove project');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useWorkbenchPin
// ---------------------------------------------------------------------------

export function useWorkbenchPin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; pinned: boolean }) => pinWorkbench(vars.id, vars.pinned),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.workbenches.list() });
      const prev = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list());
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list(), (old) =>
        old?.map((wb) =>
          wb.id === vars.id
            ? { ...wb, pinnedAt: vars.pinned ? new Date().toISOString() : null, rev: wb.rev + 1 }
            : wb,
        ),
      );
      return { prev };
    },
    onSuccess: (data) => {
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list(), (old) =>
        old?.map((wb) => (wb.id === data.id ? data : wb)),
      );
    },
    onError: (_err, vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.workbenches.list(), context.prev);
      toast.error(vars.pinned ? 'Failed to pin workbench' : 'Failed to unpin workbench');
    },
  });
}

// ---------------------------------------------------------------------------
// useWorkbenchArchive
// ---------------------------------------------------------------------------

export function useWorkbenchArchive() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => archiveWorkbench(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.workbenches.list() });
      const prev = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list());
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list(), (old) =>
        old?.filter((wb) => wb.id !== vars.id),
      );
      return { prev };
    },
    onSuccess: () => {
      // Archiving a workbench changes task visibility -- invalidate the task list
      // so the archived workbench's tasks disappear from the UI immediately.
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.all });
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.workbenches.list(), context.prev);
      toast.error('Failed to archive workbench');
    },
  });
}
