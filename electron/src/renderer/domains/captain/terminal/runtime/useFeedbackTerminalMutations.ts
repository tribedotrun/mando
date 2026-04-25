import { toast } from '#renderer/global/runtime/useFeedback';
import { useMutationFeedback } from '#renderer/global/runtime/useMutationFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import {
  useTerminalCreate as useTerminalCreateMutation,
  useTerminalDelete as useTerminalDeleteMutation,
  useWorkbenchArchive as useWorkbenchArchiveMutation,
  useWorkbenchUnarchive as useWorkbenchUnarchiveMutation,
  useWorkbenchPin as useWorkbenchPinMutation,
  useWorkbenchRename as useWorkbenchRenameMutation,
} from '#renderer/domains/captain/repo/mutations-terminal';

export function useTerminalCreate() {
  const mutation = useTerminalCreateMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to create terminal');
    },
  });
}

export function useTerminalDelete() {
  const mutation = useTerminalDeleteMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to delete terminal');
    },
  });
}

export function useWorkbenchPin() {
  const mutation = useWorkbenchPinMutation();
  return useMutationFeedback(mutation, {
    onError: (err, vars) => {
      toast.error(
        getErrorMessage(err, vars.pinned ? 'Failed to pin workbench' : 'Failed to unpin workbench'),
      );
    },
  });
}

export function useWorkbenchRename() {
  const mutation = useWorkbenchRenameMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to rename workbench');
    },
  });
}

export function useWorkbenchArchive() {
  const mutation = useWorkbenchArchiveMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to archive workbench');
    },
  });
}

export function useWorkbenchUnarchive() {
  const mutation = useWorkbenchUnarchiveMutation();
  return useMutationFeedback(mutation, {
    onError: () => {
      toast.error('Failed to unarchive workbench');
    },
  });
}
