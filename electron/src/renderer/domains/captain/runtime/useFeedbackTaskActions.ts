import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { copyToClipboard, toast } from '#renderer/global/runtime/useFeedback';
import { useSelection } from '#renderer/global/runtime/useSelection';
import { invalidateTaskDetail } from '#renderer/domains/captain/repo/taskDetailInvalidation';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import {
  useTaskAccept,
  useTaskReopen,
  useTaskRework,
  useTaskMerge,
  useTaskDelete,
  useTaskHandoff,
  useTaskStop,
  useTaskCancel,
  useTaskRetry,
  useTaskNudge,
  useTaskClarify,
} from '#renderer/domains/captain/runtime/hooks';
import type { TaskItem, TaskListResponse } from '#renderer/global/types';
import { getErrorMessage } from '#renderer/global/service/utils';

export function useTaskActions() {
  const queryClient = useQueryClient();

  const acceptMut = useTaskAccept();
  const reopenMut = useTaskReopen();
  const reworkMut = useTaskRework();
  const mergeMut = useTaskMerge();
  const deleteMut = useTaskDelete();
  const handoffMut = useTaskHandoff();
  const stopMut = useTaskStop();
  const cancelMut = useTaskCancel();
  const retryMut = useTaskRetry();
  const nudgeMut = useTaskNudge();
  const clarifyMut = useTaskClarify();

  const [reopenItem2, setReopenItem] = useState<TaskItem | null>(null);
  const [reworkItem2, setReworkItem] = useState<TaskItem | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const { selectedIds, toggleSelect, toggleSelectAll, clearSelection } = useSelection();

  const handleMerge = async (itemId: number, prNumber: number, project: string) => {
    try {
      await mergeMut.mutateAsync({ id: itemId, prNumber, project });
      void invalidateTaskDetail(queryClient, itemId);
    } catch {
      // toast handled by mutation hook
    }
  };

  const [acceptPendingId, setAcceptPendingId] = useState<number | null>(null);

  const handleAccept = async (id: number) => {
    setAcceptPendingId(id);
    try {
      await acceptMut.mutateAsync({ id });
      void invalidateTaskDetail(queryClient, id);
    } finally {
      setAcceptPendingId(null);
    }
  };

  const handleReopen = async (id: number, feedback: string) => {
    try {
      await reopenMut.mutateAsync({ id, feedback });
      void invalidateTaskDetail(queryClient, id);
    } catch {
      // toast handled by mutation hook
    }
    setReopenItem(null);
  };

  const handleRework = async (id: number, feedback: string) => {
    try {
      await reworkMut.mutateAsync({ id, feedback });
      void invalidateTaskDetail(queryClient, id);
    } catch {
      // toast handled by mutation hook
    }
    setReworkItem(null);
  };

  const handleHandoff = async (id: number) => {
    try {
      await handoffMut.mutateAsync({ id });
      void invalidateTaskDetail(queryClient, id);
      const taskData = queryClient.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const wt = taskData?.items.find((b) => b.id === id)?.worktree;
      if (wt) {
        const copied = await copyToClipboard(wt);
        if (copied) {
          toast.success('Handed off, worktree path copied. Open a terminal and run Claude there.');
        }
      } else {
        toast.success('Task handed off');
      }
    } catch (err) {
      toast.error(getErrorMessage(err, 'Handoff failed'));
    }
  };

  const handleBulkDelete = async (opts?: { close_pr?: boolean }) => {
    setDeleteError(null);
    try {
      const taskData = queryClient.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const allItems = taskData?.items ?? [];
      const ids = [...selectedIds].filter((id) => {
        const item = allItems.find((b) => b.id === id);
        return item && item.status !== 'in-progress';
      });
      await deleteMut.mutateAsync({ ids, opts: { ...opts, force: true } });
      clearSelection();
    } catch (err) {
      setDeleteError(getErrorMessage(err, 'Delete failed'));
    }
  };

  const handleCancel = (id: number) => {
    cancelMut.mutate({ id });
    void invalidateTaskDetail(queryClient, id);
  };

  const handleStop = async (id: number) => {
    try {
      await stopMut.mutateAsync({ id });
      void invalidateTaskDetail(queryClient, id);
    } catch {
      // Toast is surfaced by useTaskStop's useMutationFeedback wrapper.
    }
  };

  const handleRetry = (id: number) => {
    retryMut.mutate({ id });
    void invalidateTaskDetail(queryClient, id);
  };

  // Returns true on success so callers can decide whether to close a modal.
  // On failure the error is surfaced via toast and false is returned, allowing
  // the modal to stay open so the user does not lose their typed input.
  // invariant: mutation errors are absorbed as false return; callers use the boolean to control modal lifecycle, not to handle errors
  const handleAnswer = async (id: number, answer: string): Promise<boolean> => {
    try {
      await clarifyMut.mutateAsync({ id, mode: 'text' as const, answer });
      return true;
    } catch {
      return false;
    }
  };

  // invariant: mutation errors are absorbed as false return; callers use the boolean to control modal lifecycle, not to handle errors
  const handleNudge = async (id: number, message: string): Promise<boolean> => {
    try {
      await nudgeMut.mutateAsync({ id, message });
      return true;
    } catch {
      return false;
    }
  };

  const taskData = queryClient.getQueryData<TaskListResponse>(queryKeys.tasks.list());
  const taskItems = taskData?.items ?? [];
  const selectedItems = taskItems.filter((b) => selectedIds.has(b.id));

  return {
    selection: { selectedIds, selectedItems, toggleSelect, toggleSelectAll, clearSelection },
    merge: { handleMerge },
    reopen: {
      item: reopenItem2,
      setItem: setReopenItem,
      pending: reopenMut.isPending,
      handle: handleReopen,
    },
    rework: {
      item: reworkItem2,
      setItem: setReworkItem,
      pending: reworkMut.isPending,
      handle: handleRework,
    },
    accept: { handle: handleAccept, pendingId: acceptPendingId },
    handoff: { handle: handleHandoff },
    delete: { pending: deleteMut.isPending, error: deleteError, handleBulkDelete },
    flow: {
      handleCancel,
      handleRetry,
      handleAnswer,
      answerPending: clarifyMut.isPending,
      handleNudge,
      nudgePending: nudgeMut.isPending,
      handleStop,
      stopPending: stopMut.isPending,
    },
  };
}
