import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { useToastStore } from '#renderer/global/stores/toastStore';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useSelection } from '#renderer/domains/captain/hooks/useSelection';
import { invalidateTaskDetail } from '#renderer/queryClient';
import {
  acceptItem,
  reopenItem,
  reworkItem,
  mergePr,
  triggerTick,
  deleteItems,
  handoffItem,
  cancelItem,
  retryItem,
  answerClarificationText,
  nudgeWorker,
} from '#renderer/api';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { getErrorMessage, copyToClipboard, clarifyResultToToast } from '#renderer/utils';

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
  const [answerPending, setAnswerPending] = useState(false);
  const [nudgePending, setNudgePending] = useState(false);
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
      invalidateTaskDetail(queryClient, id);
      if (successMsg) toast().add('success', successMsg);
    } catch (err) {
      taskFetch();
      toast().add('error', getErrorMessage(err, errLabel));
    }
  }

  const handleMerge = async (itemId: number, pr: string, project: string) => {
    setMergePending(true);
    setMergeResult(null);
    optimisticUpdate(itemId, { status: 'captain-merging' });
    try {
      await mergePr(pr, project);
      setMergeResult({ ok: true, message: 'Captain will check CI and merge' });
      triggerTick().catch((err) => {
        log.warn('[tasks] post-merge tick trigger failed:', err);
      });
      taskFetch();
      invalidateTaskDetail(queryClient, itemId);
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

  const handleHandoff = async (id: number) => {
    try {
      await handoffItem(id);
      taskFetch();
      const wt = taskItems.find((b) => b.id === id)?.worktree;
      if (wt) {
        const copied = await copyToClipboard(wt);
        if (copied) {
          toast().add(
            'success',
            'Handed off, worktree path copied. Open a terminal and run Claude there.',
          );
        }
      } else {
        toast().add('success', 'Task handed off');
      }
    } catch (err) {
      toast().add('error', getErrorMessage(err, 'Handoff failed'));
    }
  };

  const handleBulkDelete = async (opts?: { close_pr?: boolean }) => {
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

  const handleCancel = (id: number) =>
    optimisticAction(id, 'canceled', () => cancelItem(id), 'Cancel failed', 'Task canceled');

  const handleRetry = (id: number) =>
    optimisticAction(
      id,
      'captain-reviewing',
      () => retryItem(id),
      'Retry failed',
      'Retry triggered',
    );

  // Returns true on success so callers can decide whether to close a modal.
  // On failure the error is surfaced via toast and false is returned, allowing
  // the modal to stay open so the user does not lose their typed input.
  const handleAnswer = async (id: number, answer: string): Promise<boolean> => {
    setAnswerPending(true);
    try {
      const result = await answerClarificationText(id, answer);
      taskFetch();
      const { variant, msg } = clarifyResultToToast(result.status);
      toast().add(variant, msg);
      return true;
    } catch (err) {
      taskFetch();
      toast().add('error', getErrorMessage(err, 'Answer failed'));
      return false;
    } finally {
      setAnswerPending(false);
    }
  };

  const handleNudge = async (id: number, message: string): Promise<boolean> => {
    setNudgePending(true);
    try {
      await nudgeWorker(id, message);
      taskFetch();
      toast().add('success', `Nudged task #${id}`);
      return true;
    } catch (err) {
      toast().add('error', getErrorMessage(err, 'Nudge failed'));
      return false;
    } finally {
      setNudgePending(false);
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
    handleHandoff,
    deleteModalOpen,
    setDeleteModalOpen,
    deleting,
    deleteError,
    handleBulkDelete,
    handleCancel,
    handleRetry,
    handleAnswer,
    answerPending,
    handleNudge,
    nudgePending,
  };
}
