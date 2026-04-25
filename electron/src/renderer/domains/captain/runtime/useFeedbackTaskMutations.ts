import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import {
  useTaskAccept as useTaskAcceptMutation,
  useTaskAsk as useTaskAskMutation,
  useTaskAskReopen as useTaskAskReopenMutation,
  useTaskAdvisor as useTaskAdvisorMutation,
  useTaskBulkCreate as useTaskBulkCreateMutation,
  useTaskCancel as useTaskCancelMutation,
  useTaskClarify as useTaskClarifyMutation,
  useTaskCreate as useTaskCreateMutation,
  useTaskDelete as useTaskDeleteMutation,
  useTaskHandoff as useTaskHandoffMutation,
  useTaskStop as useTaskStopMutation,
  useTaskMerge as useTaskMergeMutation,
  useTaskNudge as useTaskNudgeMutation,
  useTaskReopen as useTaskReopenMutation,
  useTaskRework as useTaskReworkMutation,
  useTaskRetry as useTaskRetryMutation,
  useResumeRateLimited as useResumeRateLimitedMutation,
  useStartImplementation as useStartImplementationMutation,
} from '#renderer/domains/captain/repo/mutations';
import { useAddProject as useAddProjectMutation } from '#renderer/domains/captain/repo/mutations-extra';

export function useTaskCreate() {
  const mutation = useTaskCreateMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Task created');
    },
    onError: (err) => {
      toast.error(getErrorMessage(err, 'Failed to create task'));
    },
  });
}

export function useTaskBulkCreate() {
  const mutation = useTaskBulkCreateMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (results) => {
      const ok = results.filter((result) => result.ok).length;
      const failed = results.length - ok;
      if (ok > 0) toast.success(`Created ${ok} task${ok > 1 ? 's' : ''}`);
      if (failed > 0) toast.error(`${failed} task${failed > 1 ? 's' : ''} failed`);
    },
  });
}

export function useTaskAccept() {
  const mutation = useTaskAcceptMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Accept failed');
    },
  });
}

export function useTaskCancel() {
  const mutation = useTaskCancelMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Cancel failed');
    },
  });
}

export function useTaskRetry() {
  const mutation = useTaskRetryMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Retry failed');
    },
  });
}

export function useResumeRateLimited() {
  const mutation = useResumeRateLimitedMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Rate-limit cooldown cleared');
    },
    onError: () => {
      toast.error('Resume failed');
    },
  });
}

export function useTaskHandoff() {
  return useTaskHandoffMutation();
}

export function useTaskStop() {
  const mutation = useTaskStopMutation();
  return useMutationFeedback(mutation, {
    onError: (err) => {
      toast.error(getErrorMessage(err, 'Stop failed'));
    },
  });
}

export function useAddProject() {
  const mutation = useAddProjectMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Add project failed');
    },
  });
}

export function useTaskReopen() {
  const mutation = useTaskReopenMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Task reopened');
    },
    onError: () => {
      toast.error('Reopen failed');
    },
  });
}

export function useTaskAskReopen() {
  const mutation = useTaskAskReopenMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Task reopened from Q&A');
    },
    onError: () => {
      toast.error('Reopen from Q&A failed');
    },
  });
}

export function useTaskRework() {
  const mutation = useTaskReworkMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Rework requested');
    },
    onError: () => {
      toast.error('Rework failed');
    },
  });
}

export function useTaskMerge() {
  const mutation = useTaskMergeMutation();
  return useMutationFeedback(mutation, {
    onSuccess: () => {
      toast.success('Captain will check CI and merge');
    },
    onError: () => {
      toast.error('Merge failed');
    },
  });
}

export function useTaskAsk() {
  const mutation = useTaskAskMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Ask failed');
    },
  });
}

export function useTaskAdvisor() {
  const mutation = useTaskAdvisorMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Advisor message failed');
    },
  });
}

export function useTaskNudge() {
  const mutation = useTaskNudgeMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (_data, vars) => {
      toast.success(`Nudged task #${vars.id}`);
    },
    onError: () => {
      toast.error('Nudge failed');
    },
  });
}

export function useTaskDelete() {
  const mutation = useTaskDeleteMutation();
  return useMutationFeedback(mutation, {
    onSuccess: (result) => {
      for (const warning of result.warnings ?? []) {
        toast.error(warning);
      }
    },
    onError: () => {
      toast.error('Delete failed');
    },
  });
}

export function useTaskClarify() {
  const mutation = useTaskClarifyMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Answer failed');
    },
  });
}

export function useStartImplementation() {
  const mutation = useStartImplementationMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Start implementation failed');
    },
  });
}
