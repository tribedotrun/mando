import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from '#renderer/global/runtime/useFeedback';
import log from '#renderer/global/service/logger';
import {
  createTerminal,
  deleteTerminal,
  archiveWorkbench,
  pinWorkbench,
  renameWorkbench,
  type CreateTerminalParams,
  type TerminalSessionInfo,
  type WorkbenchItem,
} from '#renderer/domains/captain/repo/terminal-api';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';

// ---------------------------------------------------------------------------
// useTerminalCreate
// ---------------------------------------------------------------------------

export function useTerminalCreate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (params: CreateTerminalParams) => toReactQuery(createTerminal(params)),
    onSuccess: (newSession) => {
      qc.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old ? [...old, newSession] : [newSession],
      );
    },
    onError: (err) => {
      log.error('useTerminalCreate', err);
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
    mutationFn: (vars: { id: string }) => deleteTerminal(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.terminals.list() });
      const prev = qc.getQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list());
      qc.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old?.filter((t) => t.id !== vars.id),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.terminals.list(), context.prev);
      log.error('useTerminalDelete', err);
      toast.error('Failed to delete terminal');
    },
  });
}

// ---------------------------------------------------------------------------
// useWorkbenchPin
// ---------------------------------------------------------------------------

export function useWorkbenchPin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; pinned: boolean }) =>
      toReactQuery(pinWorkbench(vars.id, vars.pinned)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.workbenches.all });
      const prev = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list());
      qc.setQueriesData<WorkbenchItem[]>({ queryKey: queryKeys.workbenches.all }, (old) =>
        old?.map((wb) =>
          wb.id === vars.id
            ? { ...wb, pinnedAt: vars.pinned ? new Date().toISOString() : null, rev: wb.rev + 1 }
            : wb,
        ),
      );
      return { prev };
    },
    onSuccess: (data) => {
      qc.setQueriesData<WorkbenchItem[]>({ queryKey: queryKeys.workbenches.all }, (old) =>
        old?.map((wb) => (wb.id === data.id ? data : wb)),
      );
    },
    onError: (err, vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.workbenches.list(), context.prev);
      log.error('useWorkbenchPin', err);
      toast.error(vars.pinned ? 'Failed to pin workbench' : 'Failed to unpin workbench');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useWorkbenchRename
// ---------------------------------------------------------------------------

export function useWorkbenchRename() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; title: string }) =>
      toReactQuery(renameWorkbench(vars.id, vars.title)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.workbenches.all });
      const prev = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list());
      qc.setQueriesData<WorkbenchItem[]>({ queryKey: queryKeys.workbenches.all }, (old) =>
        old?.map((wb) => (wb.id === vars.id ? { ...wb, title: vars.title, rev: wb.rev + 1 } : wb)),
      );
      return { prev };
    },
    onSuccess: (data) => {
      qc.setQueriesData<WorkbenchItem[]>({ queryKey: queryKeys.workbenches.all }, (old) =>
        old?.map((wb) => (wb.id === data.id ? data : wb)),
      );
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.workbenches.list(), context.prev);
      log.error('useWorkbenchRename', err);
      toast.error('Failed to rename workbench');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useWorkbenchArchive
// ---------------------------------------------------------------------------

export function useWorkbenchArchive() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(archiveWorkbench(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.workbenches.all });
      const prev = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list());
      const now = new Date().toISOString();
      // Remove from active list
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list(), (old) =>
        old?.filter((wb) => wb.id !== vars.id),
      );
      // Mark as archived in 'all' and 'archived' variants (keeps WorkbenchPage working)
      const markArchived = (old: WorkbenchItem[] | undefined) =>
        old?.map((wb) =>
          wb.id === vars.id ? { ...wb, archivedAt: now, pinnedAt: null, rev: wb.rev + 1 } : wb,
        );
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list('all'), markArchived);
      qc.setQueryData<WorkbenchItem[]>(queryKeys.workbenches.list('archived'), markArchived);
      return { prev };
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.all });
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.workbenches.list(), context.prev);
      log.error('useWorkbenchArchive', err);
      toast.error('Failed to archive workbench');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
    },
  });
}
