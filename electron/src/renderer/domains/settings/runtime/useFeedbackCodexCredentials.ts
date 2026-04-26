import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import {
  useCodexActiveCredential,
  useCodexCredentialActivate as useCodexCredentialActivateMutation,
  useCodexCredentialAdd as useCodexCredentialAddMutation,
} from '#renderer/domains/settings/repo/credentialsCodex';

export { useCodexActiveCredential };

export function useCodexCredentialAdd() {
  const mutation = useCodexCredentialAddMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      toast.success(`Codex account added: ${res.label}`);
    },
    onError: (err) => {
      toast.error(err.message ?? 'Failed to add Codex account');
    },
  });
}

export function useCodexCredentialActivate() {
  const mutation = useCodexCredentialActivateMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      // Toast omitted — the UI shows a confirm modal instead so the user
      // sees the list of Codex clients to restart.
    },
    onError: (err) => {
      toast.error(err.message ?? 'Failed to switch Codex account');
    },
  });
}
