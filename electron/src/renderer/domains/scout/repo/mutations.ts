import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import {
  addScoutUrl,
  bulkUpdateScout,
  bulkDeleteScout,
  updateScoutStatus,
  actOnScoutItem,
  researchScout,
  askScout,
} from '#renderer/domains/scout/repo/api';
import type { ScoutResponse } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { getErrorMessage } from '#renderer/global/service/utils';

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
// useScoutStatusUpdate
// ---------------------------------------------------------------------------

export function useScoutStatusUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; status: string }) =>
      updateScoutStatus(vars.id, vars.status),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
    },
    onError: (err) => {
      toast.error(`Status update failed: ${getErrorMessage(err, 'unknown error')}`);
    },
  });
}

// ---------------------------------------------------------------------------
// useScoutAct
// ---------------------------------------------------------------------------

export function useScoutAct() {
  return useMutation({
    mutationFn: async (vars: { id: number; project: string; prompt?: string }) =>
      actOnScoutItem(vars.id, vars.project, vars.prompt),
  });
}

// ---------------------------------------------------------------------------
// useScoutResearch
// ---------------------------------------------------------------------------

export function useScoutResearch() {
  return useMutation({
    mutationFn: async (vars: { topic: string; process?: boolean }) =>
      researchScout(vars.topic, vars.process ?? true),
    onSuccess: () => {
      toast.success('Research started');
    },
    onError: (err) => {
      toast.error(getErrorMessage(err, 'Research failed'));
    },
  });
}

// ---------------------------------------------------------------------------
// useScoutAsk
// ---------------------------------------------------------------------------

export function useScoutAsk() {
  return useMutation({
    mutationFn: async (vars: {
      id: number;
      question: string;
      sessionId?: string;
      images?: File[];
    }) => askScout(vars.id, vars.question, vars.sessionId, vars.images),
  });
}
