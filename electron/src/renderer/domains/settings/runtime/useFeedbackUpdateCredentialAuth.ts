import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import { useCredentialUpdateToken as useCredentialUpdateTokenMutation } from '#renderer/domains/settings/repo/credentials';
import { useCodexCredentialUpdateAuth as useCodexCredentialUpdateAuthMutation } from '#renderer/domains/settings/repo/credentialsCodex';

export function useCredentialUpdateToken() {
  const mutation = useCredentialUpdateTokenMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      toast.success(`Token updated: ${res.label}`);
    },
    onError: (err) => {
      toast.error(err.message ?? 'Failed to update token');
    },
  });
}

export function useCodexCredentialUpdateAuth() {
  const mutation = useCodexCredentialUpdateAuthMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      toast.success(`Codex auth updated: ${res.label}`);
      if (res.warning) {
        toast.warning(res.warning.message);
      }
    },
    onError: (err) => {
      toast.error(err.message ?? 'Failed to update Codex auth');
    },
  });
}
