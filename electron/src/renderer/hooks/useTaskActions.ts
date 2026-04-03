import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { useToastStore } from '#renderer/stores/toastStore';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useSelection } from '#renderer/hooks/useSelection';
import {
  acceptItem,
  reopenItem,
  reworkItem,
  mergePr,
  triggerTick,
  deleteItems,
  bulkUpdate,
  updateItem,
  handoffItem,
  retryItem,
  answerClarificationText,
  nudgeWorker,
} from '#renderer/api';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { getErrorMessage } from '#renderer/utils';

export function useTaskActions() {
  const taskFetch = useTaskStore((s) => s.fetch);
  const taskItems = useTaskStore((s) => s.items);
  const optimisticUpdate = useTaskStore((s) => s.optimisticUpdate);
  const queryClient = useQueryClient();

  const [mergeItem, setMergeItemRaw] = useState<TaskItem | null>(null);
  const [mergePending, setMergePending] = useState(false);
  const [mergeResult, setMergeResult] = useState<{ ok: boolean; message: string } | null>(null);

  // Clear stale mergeResult when opening a new merge modal so a previous
  // failure doesn't permanently disable the Merge button on retry.
  const setMergeItem = (item: TaskItem | null) => {
    setMergeItemRaw(item);
    if (item) setMergeResult(null);
  };
  const [reopenItem2, setReopenItem] = useState<TaskItem | null>(null);
  const [reopenPending, setReopenPending] = useState(false);
  const [reworkItem2, setReworkItem] = useState<TaskItem | null>(null);
  const [reworkPending, setReworkPending] = useState(false);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const { selectedIds, toggleSelect, toggleSelectAll, clearSelection } = useSelection();

  const toast = useToastStore.getState;

  /** Optimistic update + API call + refresh; toast on error. */
  async function optimisticAction(
    id: number,
    optimisticStatus: ItemStatus,
    fn: () => Promise<unknown>,
    errLabel: string,
    successMsg?: string,
  ): Promise<void> {
    optimisticUpdate(id, { status: optimisticStatus });
    try {
      await fn();
      taskFetch();
      queryClient.invalidateQueries({ queryKey: ['task-detail-timeline', id] });
      queryClient.invalidateQueries({ queryKey: ['task-detail-pr', id] });
      if (successMsg) toast().add('success', successMsg);
    } catch (err) {
      taskFetch();
      toast().add('error', getErrorMessage(err, errLabel));
    }
  }

  const handleMerge = async (itemId: number, pr: string, project: string) => {
    setMergePending(true);
    setMergeResult(null);
    optimisticUpdate(itemId, { status: 'captain-merging' as ItemStatus });
    try {
      await mergePr(pr, project);
      setMergeResult({ ok: true, message: 'Captain will check CI and merge' });
      triggerTick().catch((err) => {
        log.warn('[tasks] post-merge tick trigger failed:', err);
      });
      taskFetch();
      queryClient.invalidateQueries({ queryKey: ['task-detail-timeline', itemId] });
      queryClient.invalidateQueries({ queryKey: ['task-detail-pr', itemId] });
      setTimeout(() => {
        setMergeItem(null);
        setMergeResult(null);
      }, 1200);
    } catch (err) {
      optimisticUpdate(itemId, { status: 'awaiting-review' });
      setMergeResult({ ok: false, message: getErrorMessage(err, 'Merge failed') });
      taskFetch();
    } finally {
      setMergePending(false);
    }
  };

  const handleAccept = (id: number) =>
    optimisticAction(id, 'completed-no-pr', () => acceptItem(id), 'Accept failed');

  const handleReopen = async (id: number, feedback: string) => {
    setReopenPending(true);
    await optimisticAction(
      id,
      'new',
      () => reopenItem(id, feedback),
      'Reopen failed',
      'Task reopened',
    );
    setReopenPending(false);
    setReopenItem(null);
  };

  const handleRework = async (id: number, feedback: string) => {
    setReworkPending(true);
    await optimisticAction(
      id,
      'rework',
      () => reworkItem(id, feedback),
      'Rework failed',
      'Rework requested',
    );
    setReworkPending(false);
    setReworkItem(null);
  };

  const handleStatusChange = (id: number, status: ItemStatus) =>
    optimisticAction(id, status, () => updateItem(id, { status }), 'Status change failed');

  const handleHandoff = async (id: number) => {
    try {
      await handoffItem(id);
      taskFetch();
      const wt = taskItems.find((b) => b.id === id)?.worktree;
      if (wt) {
        try {
          await navigator.clipboard.writeText(wt);
          toast().add(
            'success',
            'Handed off — worktree path copied. Open a terminal and run Claude there.',
          );
        } catch {
          toast().add('success', 'Task handed off');
        }
      } else {
        toast().add('success', 'Task handed off');
      }
    } catch (err) {
      toast().add('error', getErrorMessage(err, 'Handoff failed'));
    }
  };

  const handleBulkStatus = async (status: string) => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    for (const id of ids) optimisticUpdate(id, { status: status as ItemStatus });
    try {
      await bulkUpdate(ids, { status: status as ItemStatus });
      clearSelection();
      taskFetch();
    } catch (err) {
      taskFetch();
      toast().add('error', getErrorMessage(err, 'Bulk update failed'));
    }
  };

  const handleBulkDelete = async (opts?: { close_pr?: boolean; cancel_linear?: boolean }) => {
    setDeleting(true);
    setDeleteError(null);
    try {
      const ids = [...selectedIds].filter((id) => {
        const item = taskItems.find((b) => b.id === id);
        return item && item.status !== 'in-progress';
      });
      const result = await deleteItems(ids, opts);
      clearSelection();
      taskFetch();
      setDeleteModalOpen(false);
      if (result.warnings?.length) {
        for (const w of result.warnings) {
          toast().add('error', w);
        }
      }
    } catch (err) {
      setDeleteError(getErrorMessage(err, 'Delete failed'));
    } finally {
      setDeleting(false);
    }
  };

  const handleRetry = (id: number) =>
    optimisticAction(
      id,
      'captain-reviewing' as ItemStatus,
      () => retryItem(id),
      'Retry failed',
      'Retry triggered',
    );

  const handleAnswer = async (id: number, answer: string) => {
    try {
      const result = await answerClarificationText(id, answer);
      taskFetch();
      const msgs: Record<string, [variant: 'success' | 'info', msg: string]> = {
        ready: ['success', 'Clarified — task queued'],
        clarifying: ['info', 'Still needs more info'],
        escalate: ['info', 'Escalated to captain review'],
      };
      const [variant, msg] = msgs[result.status] ?? ['success', 'Answer saved'];
      toast().add(variant, msg);
    } catch (err) {
      taskFetch();
      toast().add('error', getErrorMessage(err, 'Answer failed'));
    }
  };

  const handleNudge = async (id: number, message: string) => {
    try {
      await nudgeWorker(id, message);
      taskFetch();
      toast().add('success', `Nudged task #${id}`);
    } catch (err) {
      toast().add('error', getErrorMessage(err, 'Nudge failed'));
    }
  };

  const selectedItems = taskItems.filter((b) => selectedIds.has(b.id));

  return {
    selectedIds,
    selectedItems,
    toggleSelect,
    toggleSelectAll,
    clearSelection,
    mergeItem,
    setMergeItem,
    mergePending,
    mergeResult,
    handleMerge,
    reopenItem: reopenItem2,
    setReopenItem,
    reopenPending,
    handleReopen,
    reworkItem: reworkItem2,
    setReworkItem,
    reworkPending,
    handleRework,
    handleAccept,
    handleStatusChange,
    handleHandoff,
    handleBulkStatus,
    deleteModalOpen,
    setDeleteModalOpen,
    deleting,
    deleteError,
    handleBulkDelete,
    handleRetry,
    handleAnswer,
    handleNudge,
  };
}
