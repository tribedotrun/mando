import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  addScoutUrl,
  bulkUpdateScout,
  bulkDeleteScout,
  updateScoutStatus,
  actOnScoutItem,
  researchScout,
  askScout,
  publishScoutTelegraph,
} from '#renderer/domains/scout/repo/api';
import type { ScoutItem, ScoutItemLifecycleCommand, ScoutResponse } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';

// ---------------------------------------------------------------------------
// useScoutAdd
// ---------------------------------------------------------------------------

export function useScoutAdd() {
  return useMutation({
    mutationFn: (vars: { url: string; title?: string }) =>
      toReactQuery(addScoutUrl(vars.url, vars.title)),
    // SSE handles cache update
  });
}

// ---------------------------------------------------------------------------
// useScoutBulkUpdate
// ---------------------------------------------------------------------------

export function useScoutBulkUpdate() {
  return useMutation({
    mutationFn: (vars: { ids: number[]; command: ScoutItemLifecycleCommand }) =>
      toReactQuery(bulkUpdateScout(vars.ids, vars.command)),
    // SSE handles cache update
  });
}

// ---------------------------------------------------------------------------
// useScoutBulkDelete
// ---------------------------------------------------------------------------

export function useScoutBulkDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { ids: number[] }) => toReactQuery(bulkDeleteScout(vars.ids)),
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
    },
  });
}

// ---------------------------------------------------------------------------
// useScoutStatusUpdate
// ---------------------------------------------------------------------------

export function useScoutStatusUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; command: ScoutItemLifecycleCommand }) =>
      toReactQuery(updateScoutStatus(vars.id, vars.command)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useScoutAct
// ---------------------------------------------------------------------------

export function useScoutAct() {
  return useMutation({
    mutationFn: (vars: { id: number; project: string; prompt?: string }) =>
      toReactQuery(actOnScoutItem(vars.id, vars.project, vars.prompt)),
  });
}

// ---------------------------------------------------------------------------
// useScoutResearch
// ---------------------------------------------------------------------------

export function useScoutResearch() {
  return useMutation({
    mutationFn: (vars: { topic: string; process?: boolean }) =>
      toReactQuery(researchScout(vars.topic, vars.process ?? true)),
  });
}

// ---------------------------------------------------------------------------
// useScoutAsk
// ---------------------------------------------------------------------------

export function useScoutAsk() {
  return useMutation({
    mutationFn: (vars: { id: number; question: string; sessionId?: string; images?: File[] }) =>
      toReactQuery(askScout(vars.id, vars.question, vars.sessionId, vars.images)),
  });
}

// ---------------------------------------------------------------------------
// useScoutPublishTelegraph
// ---------------------------------------------------------------------------

export function useScoutPublishTelegraph() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(publishScoutTelegraph(vars.id)),
    onSuccess: ({ url }, vars) => {
      qc.setQueryData<ScoutItem>(queryKeys.scout.item(vars.id), (old) =>
        old ? { ...old, telegraphUrl: url } : old,
      );
      qc.setQueriesData<ScoutResponse>({ queryKey: queryKeys.scout.all }, (old) => {
        if (!old?.items) return old;
        return {
          ...old,
          items: old.items.map((item) =>
            item.id === vars.id ? { ...item, telegraphUrl: url } : item,
          ),
        };
      });
    },
  });
}
