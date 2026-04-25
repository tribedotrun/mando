import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import {
  useCredentialAdd as useCredentialAddMutation,
  useCredentialsList,
  useCredentialProbe as useCredentialProbeMutation,
  useCredentialRemove as useCredentialRemoveMutation,
  useCredentialReveal,
  type CredentialInfo,
  type CredentialRateLimitStatus,
  type CredentialWindowInfo,
} from '#renderer/domains/settings/repo/credentials';

export { useCredentialsList, useCredentialReveal };
export type { CredentialInfo, CredentialWindowInfo, CredentialRateLimitStatus };

export function useCredentialAdd() {
  const mutation = useCredentialAddMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      toast.success(`Credential added: ${res.label}`);
    },
    onError: () => {
      toast.error('Failed to add credential');
    },
  });
}

export function useCredentialRemove() {
  const mutation = useCredentialRemoveMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Credential removed');
    },
    onError: () => {
      toast.error('Failed to remove credential');
    },
  });
}

export function useCredentialProbe() {
  const mutation = useCredentialProbeMutation();
  return useMutationFeedback(mutation, {
    onError: (err) => {
      toast.error(err.message || 'Failed to probe credential');
    },
  });
}
