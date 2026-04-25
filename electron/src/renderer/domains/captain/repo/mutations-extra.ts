import { useMutation, useQueryClient } from '@tanstack/react-query';
import { endAskSession } from '#renderer/domains/captain/repo/api';
import { apiPostRouteR } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';

// ---------------------------------------------------------------------------
// useEndAskSession
// ---------------------------------------------------------------------------

export function useEndAskSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(endAskSession(vars.id)),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useAddProject
// ---------------------------------------------------------------------------

export function useAddProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { path: string }) =>
      toReactQuery(
        apiPostRouteR('postProjects', { name: undefined, path: vars.path, aliases: [] }),
      ),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}
