import React, { useCallback, useState } from 'react';
import type { Row } from '@tanstack/react-table';
import { useWorkbenchArchive } from '#renderer/domains/captain/runtime/hooks';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/global/types';
import { prLabel, prHref, prState } from '#renderer/global/service/utils';
import { PrIcon } from '#renderer/global/ui/icons';
import { StatusIcon, ACTION_LABELS } from '#renderer/global/ui/StatusIndicator';
import { Checkbox } from '#renderer/global/ui/checkbox';
import { TaskRowActions } from '#renderer/domains/captain/ui/TaskRowParts';

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
      <TaskRowActions
        item={item}
        menuOpen={menuOpen}
        onMenuOpenChange={setMenuOpen}
        archivePending={archiveWb.isPending}
        onArchive={handleArchive}
        callbacks={callbacks}
      />
    </div>
  );
});
