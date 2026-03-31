import { useState } from 'react';
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
  answerClarification,
  nudgeWorker,
} from '#renderer/api';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { getErrorMessage } from '#renderer/utils';

export function useTaskActions() {
  const taskFetch = useTaskStore((s) => s.fetch);
  const taskItems = useTaskStore((s) => s.items);

  const optimisticUpdate = useTaskStore((s) => s.optimisticUpdate);

  const [mergeItem, setMergeItem] = useState<TaskItem | null>(null);
  const [mergePending, setMergePending] = useState(false);
  const [mergeResult, setMergeResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [reopenItem2, setReopenItem] = useState<TaskItem | null>(null);
  const [reopenPending, setReopenPending] = useState(false);
  const [reworkItem2, setReworkItem] = useState<TaskItem | null>(null);
  const [reworkPending, setReworkPending] = useState(false);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const { selectedIds, toggleSelect, toggleSelectAll, clearSelection } = useSelection();

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

  const handleAccept = async (id: number) => {
    optimisticUpdate(id, { status: 'completed-no-pr' });
    try {
      await acceptItem(id);
      taskFetch();
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Accept failed'));
    }
  };

  const handleReopen = async (id: number, feedback: string) => {
    setReopenPending(true);
    optimisticUpdate(id, { status: 'new' });
    try {
      await reopenItem(id, feedback);
      taskFetch();
      useToastStore.getState().add('success', 'Task reopened');
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Reopen failed'));
    } finally {
      setReopenPending(false);
      setReopenItem(null);
    }
  };

  const handleRework = async (id: number, feedback: string) => {
    setReworkPending(true);
    optimisticUpdate(id, { status: 'rework' });
    try {
      await reworkItem(id, feedback);
      taskFetch();
      useToastStore.getState().add('success', 'Rework requested');
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Rework failed'));
    } finally {
      setReworkPending(false);
      setReworkItem(null);
    }
  };

  const handleStatusChange = async (id: number, status: ItemStatus) => {
    optimisticUpdate(id, { status });
    try {
      await updateItem(id, { status });
      taskFetch();
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Status change failed'));
    }
  };

  const handleHandoff = async (id: number) => {
    try {
      await handoffItem(id);
      taskFetch();
      const item = taskItems.find((b) => b.id === id);
      const wt = item?.worktree;
      if (wt) {
        try {
          await navigator.clipboard.writeText(wt);
          useToastStore
            .getState()
            .add(
              'success',
              'Handed off — worktree path copied. Open a terminal and run Claude there.',
            );
        } catch {
          useToastStore.getState().add('success', 'Task handed off');
        }
      } else {
        useToastStore.getState().add('success', 'Task handed off');
      }
    } catch (err) {
      useToastStore.getState().add('error', getErrorMessage(err, 'Handoff failed'));
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
      useToastStore.getState().add('error', getErrorMessage(err, 'Bulk update failed'));
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
          useToastStore.getState().add('error', w);
        }
      }
    } catch (err) {
      setDeleteError(getErrorMessage(err, 'Delete failed'));
    } finally {
      setDeleting(false);
    }
  };

  const handleRetry = async (id: number) => {
    optimisticUpdate(id, { status: 'captain-reviewing' as ItemStatus });
    try {
      await retryItem(id);
      taskFetch();
      useToastStore.getState().add('success', 'Retry triggered');
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Retry failed'));
    }
  };

  const handleAnswer = async (id: number, answer: string) => {
    try {
      const result = await answerClarification(id, answer);
      taskFetch();
      if (result.status === 'ready') {
        useToastStore.getState().add('success', 'Clarified — task queued');
      } else if (result.status === 'clarifying') {
        useToastStore.getState().add('info', 'Still needs more info');
      } else if (result.status === 'escalate') {
        useToastStore.getState().add('info', 'Escalated to captain review');
      } else {
        useToastStore.getState().add('success', 'Answer saved');
      }
    } catch (err) {
      taskFetch();
      useToastStore.getState().add('error', getErrorMessage(err, 'Answer failed'));
    }
  };

  const handleNudge = async (id: number, message: string) => {
    try {
      await nudgeWorker(id, message);
      taskFetch();
      useToastStore.getState().add('success', `Nudged task #${id}`);
    } catch (err) {
      useToastStore.getState().add('error', getErrorMessage(err, 'Nudge failed'));
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
