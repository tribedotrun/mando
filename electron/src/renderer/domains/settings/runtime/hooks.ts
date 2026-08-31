import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export { useConfig } from '#renderer/global/repo/queries';
export {
  useConfigSave,
  useProjectEdit,
  useProjectRemove,
  useProjectAdd,
} from '#renderer/global/repo/configMutations';

export {
  useCredentialsList,
  useCredentialAdd,
  useCredentialRemove,
  useCredentialSetDisabled,
  useCredentialReveal,
  useCredentialProbe,
  type CredentialInfo,
  type CredentialWindowInfo,
  type CredentialRateLimitStatus,
} from '#renderer/domains/settings/runtime/useFeedbackCredentials';

export {
  useCodexCredentialAdd,
  useCodexResetCredits,
  type CodexResetCreditsResponse,
} from '#renderer/domains/settings/runtime/useFeedbackCodexCredentials';

export {
  useCredentialUpdateToken,
  useCodexCredentialUpdateAuth,
} from '#renderer/domains/settings/runtime/useFeedbackUpdateCredentialAuth';

export { useFeedbackCodexLogin } from '#renderer/domains/settings/runtime/useFeedbackCodexLogin';

export { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
export { useLoginItemToggle } from '#renderer/domains/settings/runtime/useLoginItemToggle';
export {
  useAppVersion,
  useUpdateSystemInfo,
  useTelegramHealth,
  type TelegramHealth,
} from '#renderer/domains/settings/repo/queries';

/** Invalidates all config queries. Wraps queryKeys so UI never imports repo. */
export function useConfigInvalidate() {
  const qc = useQueryClient();
  return useCallback(() => {
    void qc.invalidateQueries({ queryKey: queryKeys.config.all });
  }, [qc]);
}
