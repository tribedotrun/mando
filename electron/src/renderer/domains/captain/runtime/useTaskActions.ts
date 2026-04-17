import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useSelection } from '#renderer/global/runtime/useSelection';
import { invalidateTaskDetail } from '#renderer/global/repo/sseCacheHelpers';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import {
  useTaskAccept,
  useTaskReopen,
  useTaskRework,
  useTaskMerge,
  useTaskDelete,
  useTaskHandoff,
  useTaskCancel,
  useTaskRetry,
  useTaskNudge,
  useTaskClarify,
} from '#renderer/domains/captain/runtime/hooks';
import type { TaskItem, TaskListResponse } from '#renderer/global/types';
import { copyToClipboard, getErrorMessage } from '#renderer/global/service/utils';

export function useTaskActions() {
  const queryClient = useQueryClient();

  const acceptMut = useTaskAccept();
  const reopenMut = useTaskReopen();
  const reworkMut = useTaskRework();
  const mergeMut = useTaskMerge();
  const deleteMut = useTaskDelete();
  const handoffMut = useTaskHandoff();
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

  const handleRetry = (id: number) => {
    retryMut.mutate({ id });
    void invalidateTaskDetail(queryClient, id);
  };

  // Returns true on success so callers can decide whether to close a modal.
  // On failure the error is surfaced via toast and false is returned, allowing
  // the modal to stay open so the user does not lose their typed input.
  const handleAnswer = async (id: number, answer: string): Promise<boolean> => {
    try {
      await clarifyMut.mutateAsync({ id, mode: 'text' as const, answer });
      return true;
    } catch {
      return false;
    }
  };

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
    selectedIds,
    selectedItems,
    toggleSelect,
    toggleSelectAll,
    clearSelection,
    handleMerge,
    reopenItem: reopenItem2,
    setReopenItem,
    reopenPending: reopenMut.isPending,
    handleReopen,
    reworkItem: reworkItem2,
    setReworkItem,
    reworkPending: reworkMut.isPending,
    handleRework,
    handleAccept,
    acceptPendingId,
    handleHandoff,
    deleting: deleteMut.isPending,
    deleteError,
    handleBulkDelete,
    handleCancel,
    handleRetry,
    handleAnswer,
    answerPending: clarifyMut.isPending,
    handleNudge,
    nudgePending: nudgeMut.isPending,
  };
}
