import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import {
  useCodexCredentialAdd as useCodexCredentialAddMutation,
  useCodexResetCredits,
  type CodexResetCreditsResponse,
} from '#renderer/domains/settings/repo/credentialsCodex';

export { useCodexResetCredits };
export type { CodexResetCreditsResponse };

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
