import { useIsMutating } from '@tanstack/react-query';
import {
  useClaudeDesktopAdopt,
  useClaudeDesktopOpen,
  useClaudeDesktopProfileStatus,
  useClaudeDesktopSetup,
  useClaudeDesktopSessionImport,
  useClaudeDesktopSessionPreview,
} from '#renderer/domains/settings/repo/claudeDesktopApp';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';

export function useClaudeDesktopProfile(credentialId: number) {
  const status = useClaudeDesktopProfileStatus(credentialId);
  const pending = useIsMutating({ mutationKey: queryKeys.claudeDesktopApp.action() }) > 0;
  const setup = useMutationFeedback(useClaudeDesktopSetup(), {
    onSuccess: () => toast.success('Claude Desktop opened. Complete sign-in in its window.'),
    onError: (error) => toast.error(error.message || 'Could not set up Claude Desktop'),
  });
  const open = useMutationFeedback(useClaudeDesktopOpen(), {
    onError: (error) => toast.error(error.message || 'Could not open Claude Desktop'),
  });
  const adopt = useMutationFeedback(useClaudeDesktopAdopt(), {
    onSuccess: () => toast.success('Existing Claude Desktop profile linked'),
    onError: (error) => toast.error(error.message || 'Could not link Claude Desktop profile'),
  });
  const importSessions = useMutationFeedback(useClaudeDesktopSessionImport(), {
    onSuccess: (result) =>
      toast.success(`Imported ${result.imported} local sessions; skipped ${result.skipped}`),
    onError: (error) => toast.error(error.message || 'Could not import local sessions'),
  });
  const previewSessions = useMutationFeedback(useClaudeDesktopSessionPreview(), {
    onError: (error) => toast.error(error.message || 'Could not preview local sessions'),
  });
  return { status, pending, setup, open, adopt, importSessions, previewSessions };
}
