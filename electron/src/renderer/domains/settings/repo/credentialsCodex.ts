import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiPostRouteR } from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';

const LIST_KEY = queryKeys.credentials.all;

export function useCodexCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, authJson }: { label: string; authJson: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsCodex', { label, authJson })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: LIST_KEY });
    },
  });
}
