import React, { useState, useCallback, useRef } from 'react';
import { useTaskActions } from '#renderer/domains/captain/hooks/useTaskActions';
import { useFilteredTasks } from '#renderer/domains/captain/hooks/useFilteredTasks';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import { TaskTable } from '#renderer/domains/captain/components/TaskTable';
import { StatusFilter } from '#renderer/domains/captain/components/StatusFilter';
import { MetricsRow } from '#renderer/domains/captain/components/MetricsRow';
import { BulkBar } from '#renderer/domains/captain/components/BulkBar';
import { DeleteModal } from '#renderer/domains/captain/components/DeleteModal';
import { MergeModal } from '#renderer/domains/captain/components/MergeModal';
import { FeedbackModal } from '#renderer/domains/captain/components/FeedbackModal';
import { TaskAsk } from '#renderer/domains/captain/components/TaskAsk';
import type { TaskItem, WorkerDetail } from '#renderer/types';
import { canMerge, canRestart, canRework, indexNext, indexPrev } from '#renderer/utils';

interface Props {
  projectFilter: string | null;
  onCreateTask?: () => void;
  onOpenDetail?: (item: TaskItem) => void;
  active?: boolean;
}

export function CaptainView({
  projectFilter,
  onCreateTask,
  onOpenDetail,
  active = true,
}: Props): React.ReactElement {
  const [askItem, setAskItem] = useState<TaskItem | null>(null);
  const [answerItem, setAnswerItem] = useState<TaskItem | null>(null);
  const [nudgeWorker, setNudgeWorker] = useState<WorkerDetail | null>(null);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const actions = useTaskActions();

  const handleAnswerItem = (item: TaskItem) => setAnswerItem(item);
  const handleNudgeWorker = (worker: WorkerDetail) => setNudgeWorker(worker);

  const visibleItems = useFilteredTasks(projectFilter);

  // Reset keyboard focus when the user switches projects, using the
  // "derive state from props" ref pattern (the codebase bans useEffect).
  const prevProjectFilterRef = useRef(projectFilter);
  if (prevProjectFilterRef.current !== projectFilter) {
    prevProjectFilterRef.current = projectFilter;
    if (focusedIndex !== -1) setFocusedIndex(-1);
  }

  // Clamp focusedIndex inline — derived from visibleItems.length
  const clampedFocusedIndex =
    visibleItems.length === 0
      ? -1
      : focusedIndex >= visibleItems.length
        ? visibleItems.length - 1
        : focusedIndex;

  const hasModal =
    !!askItem ||
    !!answerItem ||
    !!nudgeWorker ||
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
          setFocusedIndex((i) => indexNext(i, visibleItems.length - 1));
          break;
        }
        case 'k': {
          e.preventDefault();
          setFocusedIndex((i) => indexPrev(i));
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
      }
    },
    [hasModal, visibleItems, clampedFocusedIndex, onCreateTask, onOpenDetail, actions],
  );

  useViewKeyHandler(handleKey, active);

  if (askItem) {
    return <TaskAsk item={askItem} onBack={() => setAskItem(null)} />;
  }

  return (
    <div className="flex h-full flex-col">
      <MetricsRow
        onNudge={handleNudgeWorker}
        onStopWorker={(worker) => actions.handleHandoff(worker.id)}
      />
      <StatusFilter projectFilter={projectFilter} />

      <div className="min-h-0 flex-1 pt-1">
        <TaskTable
          selectedIds={actions.selectedIds}
          onToggleSelect={actions.toggleSelect}
          onMerge={actions.setMergeItem}
          onReopen={actions.setReopenItem}
          onRework={actions.setReworkItem}
          onAsk={setAskItem}
          onAccept={actions.handleAccept}
          acceptPendingId={actions.acceptPendingId}
          onHandoff={actions.handleHandoff}
          onCancel={actions.handleCancel}
          onRetry={actions.handleRetry}
          onAnswer={handleAnswerItem}
          onOpenDetail={onOpenDetail}
          projectFilter={projectFilter}
          focusedIndex={clampedFocusedIndex}
        />
      </div>

      <BulkBar
        count={actions.selectedIds.size}
        onDelete={() => actions.setDeleteModalOpen(true)}
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
      {answerItem && (
        <FeedbackModal
          testId="answer-modal"
          title="Answer"
          subtitle={answerItem.title}
          placeholder="Type your answer…"
          buttonLabel="Send"
          pendingLabel="Sending…"
          isPending={actions.answerPending}
          onSubmit={async (answer) => {
            const ok = await actions.handleAnswer(answerItem.id, answer);
            if (ok) setAnswerItem(null);
          }}
          onCancel={() => setAnswerItem(null)}
        />
      )}
      {nudgeWorker && (
        <FeedbackModal
          testId="nudge-modal"
          title="Nudge worker"
          subtitle={nudgeWorker.title}
          placeholder="Nudge message"
          initialValue="Keep going. Ship the next concrete step."
          buttonLabel="Nudge"
          pendingLabel="Nudging…"
          isPending={actions.nudgePending}
          onSubmit={async (msg) => {
            const ok = await actions.handleNudge(nudgeWorker.id, msg);
            if (ok) setNudgeWorker(null);
          }}
          onCancel={() => setNudgeWorker(null)}
        />
      )}
    </div>
  );
}
