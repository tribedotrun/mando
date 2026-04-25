import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import {
  useScoutAct as useScoutActMutation,
  useScoutAdd as useScoutAddMutation,
  useScoutAsk as useScoutAskMutation,
  useScoutBulkDelete as useScoutBulkDeleteMutation,
  useScoutBulkUpdate as useScoutBulkUpdateMutation,
  useScoutPublishTelegraph as useScoutPublishTelegraphMutation,
  useScoutResearch as useScoutResearchMutation,
  useScoutStatusUpdate as useScoutStatusUpdateMutation,
} from '#renderer/domains/scout/repo/mutations';

export function useScoutAdd() {
  const mutation = useScoutAddMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to add scout item');
    },
  });
}

export function useScoutBulkUpdate() {
  const mutation = useScoutBulkUpdateMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Bulk update failed');
    },
  });
}

export function useScoutBulkDelete() {
  const mutation = useScoutBulkDeleteMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Bulk delete failed');
    },
  });
}

export function useScoutStatusUpdate() {
  const mutation = useScoutStatusUpdateMutation();
  return useMutationFeedback(mutation, {
    onError: (err) => {
      toast.error(`Status update failed: ${getErrorMessage(err, 'unknown error')}`);
    },
  });
}

export function useScoutAct() {
  return useScoutActMutation();
}

export function useScoutResearch() {
  const mutation = useScoutResearchMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Research started');
    },
    onError: (err) => {
      toast.error(getErrorMessage(err, 'Research failed'));
    },
  });
}

export function useScoutAsk() {
  return useScoutAskMutation();
}

export function useScoutPublishTelegraph() {
  const mutation = useScoutPublishTelegraphMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Published to Telegraph');
    },
    onError: (err) => {
      toast.error(getErrorMessage(err, 'Telegraph publish failed'));
    },
  });
}
