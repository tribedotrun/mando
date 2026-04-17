import React, { useCallback, useState } from 'react';
import type { Row } from '@tanstack/react-table';
import { useWorkbenchArchive } from '#renderer/domains/captain/runtime/hooks';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/global/types';
import {
  prLabel,
  prHref,
  prState,
  canAnswer,
  canMerge,
  canReopen,
  canAskTerminal,
} from '#renderer/global/service/utils';
import { ArchiveBtn, MergeBtn, MoreIcon } from '#renderer/domains/captain/ui/TaskIcons';
import { PrIcon } from '#renderer/global/ui/icons';
import { StatusIcon, ACTION_LABELS } from '#renderer/global/ui/StatusIndicator';
import { ActionBtn, TaskOverflowMenu } from '#renderer/domains/captain/ui/TaskActions';
import { Checkbox } from '#renderer/global/ui/checkbox';
import { Button } from '#renderer/global/ui/button';

export interface TaskRowCallbacks {
  onToggleSelect: (id: number) => void;
  onMerge: (item: TaskItem) => void;
  onReopen: (item: TaskItem) => void;
  onRework: (item: TaskItem) => void;
  onAsk: (item: TaskItem) => void;
  onAccept: (id: number) => void;
  acceptPendingId?: number | null;
  onHandoff: (id: number) => void;
  onCancel: (id: number) => void;
  onRetry: (id: number) => void;
  onAnswer: (item: TaskItem) => void;
  onOpenDetail?: (item: TaskItem) => void;
}

interface TaskRowProps {
  row: Row<TaskItem>;
  focused: boolean;
  scrollRef?: (node: HTMLElement | null) => void;
  callbacks: TaskRowCallbacks;
}

export const TaskRow = React.memo(function TaskRow({
  row,
  focused,
  scrollRef,
  callbacks,
}: TaskRowProps): React.ReactElement {
  const item = row.original;
  const selected = row.getIsSelected();
  const archiveWb = useWorkbenchArchive();
  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const [menuOpen, setMenuOpen] = useState(false);

  const handleArchive = useCallback(() => {
    if (item.workbench_id) archiveWb.mutate({ id: item.workbench_id });
  }, [item.workbench_id, archiveWb]);

  return (
    <div
      ref={scrollRef}
      data-testid="task-row"
      data-focused={focused || undefined}
      className={`group relative flex cursor-pointer items-center gap-2.5 rounded px-3 py-2 ${selected ? 'bg-accent' : 'bg-card'} ${isFinalized ? 'opacity-55' : ''} ${focused ? 'outline-2 outline-ring -outline-offset-2' : ''} ${menuOpen ? 'z-20' : ''}`}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('[data-actions]')) return;
        callbacks.onOpenDetail?.(item);
      }}
    >
      {/* Col: checkbox, overlays the status icon on hover/select */}
      <span className="status-icon-wrapper relative h-4 w-4 shrink-0">
        <span
          className={`absolute inset-0 flex items-center justify-center transition-opacity ${selected ? 'opacity-0' : 'group-hover:opacity-0'}`}
        >
          <StatusIcon status={item.status} />
        </span>
        <span
          className={`absolute inset-0 z-[1] flex items-center justify-center transition-opacity group-hover:opacity-100 ${selected ? 'opacity-100' : 'opacity-0'}`}
        >
          <Checkbox
            checked={selected}
            aria-label={`Select "${item.title || item.original_prompt || 'Untitled task'}"`}
            onCheckedChange={() => callbacks.onToggleSelect(item.id)}
            onClick={(e) => e.stopPropagation()}
          />
        </span>
      </span>

      {/* Col: title + badges, title truncates, badges never compress */}
      <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[14px] leading-[18px]">
        <span className={`min-w-0 truncate ${isFinalized ? 'text-text-3' : 'text-foreground'}`}>
          {ACTION_LABELS[item.status] && (
            <span
              className="mr-1.5 text-[12px] font-medium"
              style={{ color: ACTION_LABELS[item.status].color }}
            >
              {ACTION_LABELS[item.status].label}
              {' \u00b7 '}
            </span>
          )}
          {item.session_ids?.ask && (
            <span className="mr-1.5 text-[11px] font-medium text-muted-foreground opacity-85">
              Q&A
              {' \u00b7 '}
            </span>
          )}
          {item.title || item.original_prompt || 'Untitled task'}
        </span>
        {item.pr_number && (item.github_repo || item.project) && (
          <a
            href={prHref(item.pr_number, (item.github_repo ?? item.project)!)}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex shrink-0 items-center gap-0.5 rounded bg-secondary px-1 font-mono text-[11px] text-text-3 no-underline hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            <PrIcon state={prState(item.status)} />
            {prLabel(item.pr_number)}
          </a>
        )}
      </span>

      {/* Actions, inline flex item, hidden until hover via CSS */}
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
          <ArchiveBtn onClick={handleArchive} pending={archiveWb.isPending} />
        )}
        {canAskTerminal(item) && <ActionBtn label="Ask" onClick={() => callbacks.onAsk(item)} />}
        {!isFinalized && (
          <TaskOverflowMenu
            item={item}
            open={menuOpen}
            onOpenChange={setMenuOpen}
            onRework={() => callbacks.onRework(item)}
            onHandoff={() => callbacks.onHandoff(item.id)}
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
    </div>
  );
});
