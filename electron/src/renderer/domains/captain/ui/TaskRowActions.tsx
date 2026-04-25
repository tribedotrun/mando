import React from 'react';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/global/types';
import { canAnswer, canMerge, canReopen, canAskTerminal } from '#renderer/global/service/utils';
import { ArchiveBtn, MergeBtn, MoreIcon } from '#renderer/domains/captain/ui/TaskIcons';
import { ActionBtn, TaskOverflowMenu } from '#renderer/domains/captain/ui/TaskActions';
import { Button } from '#renderer/global/ui/primitives/button';
import type { TaskRowCallbacks } from '#renderer/domains/captain/ui/TaskRow';

interface TaskRowActionsProps {
  item: TaskItem;
  menuOpen: boolean;
  onMenuOpenChange: (open: boolean) => void;
  archivePending: boolean;
  onArchive: () => void;
  callbacks: TaskRowCallbacks;
}

export function TaskRowActions({
  item,
  menuOpen,
  onMenuOpenChange,
  archivePending,
  onArchive,
  callbacks,
}: TaskRowActionsProps): React.ReactElement {
  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  return (
    <div
      data-actions
      data-menu-open={menuOpen || undefined}
      className="action-zone flex shrink-0 items-center gap-1.5"
      onClick={(e) => e.stopPropagation()}
    >
      {canMerge(item) && <MergeBtn onClick={() => callbacks.onMerge(item)} />}
      {item.status === 'awaiting-review' && !item.pr_number && (
        <ActionBtn
          label="Accept"
          onClick={() => callbacks.onAccept(item.id)}
          testId="accept-btn"
          pending={callbacks.acceptPendingId === item.id}
        />
      )}
      {canAnswer(item) && (
        <ActionBtn label="Answer" onClick={() => callbacks.onAnswer(item)} testId="answer-btn" />
      )}
      {canReopen(item) && (
        <ActionBtn label="Reopen" onClick={() => callbacks.onReopen(item)} testId="reopen-btn" />
      )}
      {isFinalized && item.workbench_id && (
        <ArchiveBtn onClick={onArchive} pending={archivePending} />
      )}
      {canAskTerminal(item) && <ActionBtn label="Ask" onClick={() => callbacks.onAsk(item)} />}
      {!isFinalized && (
        <TaskOverflowMenu
          item={item}
          open={menuOpen}
          onOpenChange={onMenuOpenChange}
          onRework={() => callbacks.onRework(item)}
          onHandoff={() => callbacks.onHandoff(item.id)}
          onStop={() => callbacks.onStop(item.id)}
          onCancel={() => callbacks.onCancel(item.id)}
          onRetry={() => callbacks.onRetry(item.id)}
          onAnswer={() => callbacks.onAnswer(item)}
        >
          <Button variant="outline" size="icon-xs" aria-label="More actions">
            <MoreIcon />
          </Button>
        </TaskOverflowMenu>
      )}
    </div>
  );
}
