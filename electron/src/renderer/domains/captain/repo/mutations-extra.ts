import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { endAskSession } from '#renderer/domains/captain/repo/api';
import { apiPost } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';

// ---------------------------------------------------------------------------
// useEndAskSession
// ---------------------------------------------------------------------------

export function useEndAskSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => endAskSession(vars.id),
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
    mutationFn: async (vars: { path: string }) => apiPost('/api/projects', { path: vars.path }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
    onError: () => {
      toast.error('Failed to add project');
    },
  });
}
