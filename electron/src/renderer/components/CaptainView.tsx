import React, { useState, useCallback } from 'react';
import { useTaskActions } from '#renderer/hooks/useTaskActions';
import { useFilteredTasks } from '#renderer/hooks/useFilteredTasks';
import { useViewKeyHandler } from '#renderer/hooks/useKeyboardShortcuts';
import { TaskTable } from '#renderer/components/TaskTable';
import { StatusFilter } from '#renderer/components/StatusFilter';
import { MetricsRow } from '#renderer/components/MetricsRow';
import { BulkBar } from '#renderer/components/BulkBar';
import { DeleteModal } from '#renderer/components/DeleteModal';
import { MergeModal } from '#renderer/components/MergeModal';
import { FeedbackModal } from '#renderer/components/FeedbackModal';
import { TaskAsk } from '#renderer/components/TaskAsk';
import type { TaskItem } from '#renderer/types';
import type { WorkerDetail } from '#renderer/types';
import { canMerge, canRestart, canRework } from '#renderer/utils';

interface Props {
  projectFilter: string | null;
  onCreateTask?: () => void;
  onOpenDetail?: (item: TaskItem) => void;
}

export function CaptainView({
  projectFilter,
  onCreateTask,
  onOpenDetail,
}: Props): React.ReactElement {
  const [askItem, setAskItem] = useState<TaskItem | null>(null);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const actions = useTaskActions();

  const handleHandoffItem = (item: TaskItem) => {
    actions.handleHandoff(item.id);
  };
  const handleCancelItem = (item: TaskItem) => {
    actions.handleStatusChange(item.id, 'canceled');
  };
  const handleRetryItem = (item: TaskItem) => {
    actions.handleRetry(item.id);
  };
  const handleAcceptItem = (item: TaskItem) => {
    actions.handleAccept(item.id);
  };
  const handleAnswerItem = (item: TaskItem) => {
    const answer = window.prompt(`Answer for "${item.title}":`);
    if (answer) actions.handleAnswer(item.id, answer);
  };
  const handleNudgeWorker = (worker: WorkerDetail) => {
    const message = window.prompt(
      `Nudge message for "${worker.title}"`,
      'Keep going. Ship the next concrete step.',
    );
    if (message) actions.handleNudge(worker.id, message);
  };

  const visibleItems = useFilteredTasks(projectFilter);

  // Clamp focusedIndex inline — derived from visibleItems.length
  const clampedFocusedIndex =
    visibleItems.length === 0
      ? -1
      : focusedIndex >= visibleItems.length
        ? visibleItems.length - 1
        : focusedIndex;

  const hasModal =
    !!askItem ||
    !!actions.mergeItem ||
    !!actions.reopenItem ||
    !!actions.reworkItem ||
    actions.deleteModalOpen;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      if (hasModal) return;

      switch (key) {
        case 'j': {
          e.preventDefault();
          setFocusedIndex((i) => Math.min(i + 1, visibleItems.length - 1));
          break;
        }
        case 'k': {
          e.preventDefault();
          setFocusedIndex((i) => Math.max(i - 1, 0));
          break;
        }
        case 'c': {
          e.preventDefault();
          onCreateTask?.();
          break;
        }
        case 'x':
        case 'Escape': {
          if (clampedFocusedIndex >= 0) {
            e.preventDefault();
            setFocusedIndex(-1);
          }
          break;
        }
        case 'Enter': {
          const item = visibleItems[clampedFocusedIndex];
          if (item) {
            e.preventDefault();
            onOpenDetail?.(item);
          }
          break;
        }
        case 'm': {
          const item = visibleItems[clampedFocusedIndex];
          if (item && canMerge(item)) {
            e.preventDefault();
            actions.setMergeItem(item);
          }
          break;
        }
        case 'r': {
          const item = visibleItems[clampedFocusedIndex];
          if (item) {
            e.preventDefault();
            if (canRework(item)) actions.setReworkItem(item);
            else if (canRestart(item)) actions.setReopenItem(item);
          }
          break;
        }
        case 's': {
          const item = visibleItems[clampedFocusedIndex];
          if (item) {
            e.preventDefault();
            window.dispatchEvent(new CustomEvent('mando:edit-status', { detail: { id: item.id } }));
          }
          break;
        }
      }
    },
    [hasModal, visibleItems, clampedFocusedIndex, onCreateTask, onOpenDetail, actions],
  );

  useViewKeyHandler(handleKey);

  if (askItem) {
    return <TaskAsk item={askItem} onBack={() => setAskItem(null)} />;
  }

  return (
    <div className="flex flex-col" style={{ height: '100%' }}>
      <MetricsRow
        onNudge={handleNudgeWorker}
        onStopWorker={(worker) => actions.handleStatusChange(worker.id, 'canceled')}
      />
      <StatusFilter />

      <div className="min-h-0 flex-1" style={{ paddingTop: 4 }}>
        <TaskTable
          selectedIds={actions.selectedIds}
          onToggleSelect={actions.toggleSelect}
          onToggleSelectAll={actions.toggleSelectAll}
          onMerge={actions.setMergeItem}
          onReopen={actions.setReopenItem}
          onRework={actions.setReworkItem}
          onAsk={setAskItem}
          onAccept={handleAcceptItem}
          onHandoff={handleHandoffItem}
          onCancel={handleCancelItem}
          onRetry={handleRetryItem}
          onAnswer={handleAnswerItem}
          onOpenDetail={onOpenDetail}
          projectFilter={projectFilter}
          focusedIndex={clampedFocusedIndex}
        />
      </div>

      <BulkBar
        count={actions.selectedIds.size}
        onDelete={() => actions.setDeleteModalOpen(true)}
        onBulkStatus={actions.handleBulkStatus}
        onCancel={actions.clearSelection}
      />

      {actions.mergeItem && (
        <MergeModal
          item={actions.mergeItem}
          onConfirm={actions.handleMerge}
          onCancel={() => actions.setMergeItem(null)}
          pending={actions.mergePending}
          result={actions.mergeResult}
        />
      )}
      {actions.reopenItem && (
        <FeedbackModal
          testId="reopen-modal"
          title="Reopen"
          subtitle={actions.reopenItem.title}
          placeholder="What changes are needed?"
          buttonLabel="Reopen"
          pendingLabel="Reopening..."
          isPending={actions.reopenPending}
          onSubmit={(fb) => actions.handleReopen(actions.reopenItem!.id, fb)}
          onCancel={() => actions.setReopenItem(null)}
        />
      )}
      {actions.reworkItem && (
        <FeedbackModal
          testId="rework-modal"
          title="Rework this task"
          subtitle={actions.reworkItem.title}
          label="INSTRUCTIONS (OPTIONAL)"
          placeholder="What should the agent do differently?"
          buttonLabel="Rework"
          pendingLabel="Reworking..."
          isPending={actions.reworkPending}
          onSubmit={(fb) => actions.handleRework(actions.reworkItem!.id, fb)}
          onCancel={() => actions.setReworkItem(null)}
        />
      )}
      {actions.deleteModalOpen && (
        <DeleteModal
          items={actions.selectedItems}
          deleting={actions.deleting}
          error={actions.deleteError}
          onConfirm={actions.handleBulkDelete}
          onCancel={() => actions.setDeleteModalOpen(false)}
        />
      )}
    </div>
  );
}
