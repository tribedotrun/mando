import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import {
  useCodexCredentialAdd as useCodexCredentialAddMutation,
  useCodexCredentialWarmup as useCodexCredentialWarmupMutation,
  useCodexResetCredits,
  type CodexResetCreditsResponse,
  type CodexWarmupResponse,
} from '#renderer/domains/settings/repo/credentialsCodex';

export { useCodexResetCredits };
export type { CodexResetCreditsResponse, CodexWarmupResponse };

export function useCodexCredentialWarmup() {
  const mutation = useCodexCredentialWarmupMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      const model = res.model ?? 'default model';
      toast.success(`Usage clock started for ${res.label} (${model})`);
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to start the Codex usage clock');
    },
  });
}

export function useCodexCredentialAdd() {
  const mutation = useCodexCredentialAddMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (res) => {
      toast.success(`Codex account added: ${res.label}`);
      if (res.warning) {
        toast.warning(res.warning.message);
      }
    },
    onError: (err) => {
      toast.error(err.message ?? 'Failed to add Codex account');
    },
  });
}
