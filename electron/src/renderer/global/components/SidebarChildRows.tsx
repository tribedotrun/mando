import React from 'react';
import { Archive, Terminal, Pin } from 'lucide-react';
import { FINALIZED_STATUSES } from '#renderer/types';
import { compactRelativeTime } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import {
  WorkbenchContextMenu,
  WorkbenchRenameInput,
  type WorkbenchMenuTarget,
} from '#renderer/global/components/WorkbenchContextMenu';

export function TaskRow({
  task,
  onOpenTask,
  onArchiveWorkbench,
  onPinWorkbench,
}: {
  task: import('#renderer/types').TaskItem;
  onOpenTask?: (taskId: number, workbenchId?: number) => void;
  onArchiveWorkbench?: (id: number) => void;
  onPinWorkbench?: (id: number) => void;
}): React.ReactElement {
  const canArchive =
    task.workbench_id != null && onArchiveWorkbench && FINALIZED_STATUSES.includes(task.status);
  const canPin = task.workbench_id != null && onPinWorkbench;
  return (
    <button
      onClick={() => onOpenTask?.(task.id, task.workbench_id ?? undefined)}
      className="group flex w-full items-center gap-2 rounded px-2 py-1 text-left text-[12px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      style={{ background: 'none', border: 'none', cursor: 'pointer' }}
    >
      <span className="relative shrink-0 translate-y-px">
        <span className={canPin ? 'group-hover:invisible' : ''}>
          <StatusIcon status={task.status} />
        </span>
        {canPin && (
          <span
            role="button"
            tabIndex={-1}
            title="Pin workbench"
            onClick={(e) => {
              e.stopPropagation();
              onPinWorkbench!(task.workbench_id!);
            }}
            className="absolute inset-0 hidden items-center justify-center text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
            style={{ cursor: 'pointer' }}
          >
            <Pin size={12} />
          </span>
        )}
      </span>
      <span className="min-w-0 flex-1 truncate">
        {task.title || task.original_prompt || 'Untitled task'}
      </span>
      <span className="flex shrink-0 items-center gap-1">
        {(task.last_activity_at || task.created_at) && (
          <span className={`text-[11px] text-text-3 ${canArchive ? 'group-hover:hidden' : ''}`}>
            {compactRelativeTime((task.last_activity_at || task.created_at)!)}
          </span>
        )}
        {canArchive && (
          <span
            role="button"
            tabIndex={-1}
            title="Archive workbench"
            onClick={(e) => {
              e.stopPropagation();
              onArchiveWorkbench!(task.workbench_id!);
            }}
            className="hidden shrink-0 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
            style={{ cursor: 'pointer' }}
          >
            <Archive size={11} />
          </span>
        )}
      </span>
    </button>
  );
}

export function WorkbenchRow({
  projectName,
  wb,
  activeWorktreeCwd,
  renamingWbId,
  setRenamingWbId,
  onOpenWorktree,
  onArchiveWorkbench,
  onPinWorkbench,
  onUnpinWorkbench,
  onRenameWorkbench,
  onOpenWorkbenchInFinder,
  onCopyWorkbenchPath,
}: {
  projectName: string;
  wb: import('#renderer/api-terminal').WorkbenchItem;
  activeWorktreeCwd: string | null;
  renamingWbId: number | null;
  setRenamingWbId: (id: number | null) => void;
  onOpenWorktree?: (worktree: { project: string; cwd: string }) => void;
  onArchiveWorkbench?: (id: number) => void;
  onPinWorkbench?: (id: number) => void;
  onUnpinWorkbench?: (id: number) => void;
  onRenameWorkbench?: (id: number, title: string) => void;
  onOpenWorkbenchInFinder?: (worktree: string) => void;
  onCopyWorkbenchPath?: (worktree: string) => void;
}): React.ReactElement {
  const isActive = activeWorktreeCwd === wb.worktree;
  const canPinWt = onPinWorkbench != null;
  const activity = wb.lastActivityAt || wb.createdAt;
  const canContextMenu =
    onRenameWorkbench &&
    onArchiveWorkbench &&
    onPinWorkbench &&
    onUnpinWorkbench &&
    onOpenWorkbenchInFinder &&
    onCopyWorkbenchPath;
  const target: WorkbenchMenuTarget | null = canContextMenu
    ? {
        id: wb.id,
        title: wb.title,
        worktree: wb.worktree,
        pinnedAt: wb.pinnedAt ?? null,
        archivedAt: wb.archivedAt ?? null,
      }
    : null;

  if (renamingWbId === wb.id) {
    return (
      <WorkbenchRenameInput
        initialValue={wb.title}
        onCommit={(newTitle) => {
          setRenamingWbId(null);
          const trimmed = newTitle.trim();
          if (trimmed && trimmed !== wb.title) {
            onRenameWorkbench?.(wb.id, trimmed);
          }
        }}
        onCancel={() => setRenamingWbId(null)}
      />
    );
  }

  const rowButton = (
    <button
      onClick={() => onOpenWorktree?.({ project: projectName, cwd: wb.worktree })}
      className={`sidebar-workbench-item group flex w-full items-center gap-2 rounded px-2 py-1 text-left text-[12px] transition-colors hover:bg-muted hover:text-foreground ${isActive ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground'}`}
      style={{
        background: isActive ? undefined : 'none',
        border: 'none',
        cursor: 'pointer',
      }}
    >
      <span className="relative inline-flex w-4 shrink-0 items-center justify-center translate-y-px">
        <Terminal size={11} className={`text-text-3 ${canPinWt ? 'group-hover:invisible' : ''}`} />
        {canPinWt && (
          <span
            role="button"
            tabIndex={-1}
            title="Pin workbench"
            onClick={(e) => {
              e.stopPropagation();
              onPinWorkbench!(wb.id);
            }}
            className="absolute inset-0 hidden items-center justify-center text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
            style={{ cursor: 'pointer' }}
          >
            <Pin size={12} />
          </span>
        )}
      </span>
      <span className="min-w-0 flex-1 truncate">{wb.title}</span>
      <span className="flex shrink-0 items-center gap-1">
        {activity && (
          <span
            className={`text-[11px] text-text-3 ${onArchiveWorkbench ? 'group-hover:hidden' : ''}`}
          >
            {compactRelativeTime(activity)}
          </span>
        )}
        {onArchiveWorkbench && (
          <span
            role="button"
            tabIndex={-1}
            title="Archive workbench"
            onClick={(e) => {
              e.stopPropagation();
              onArchiveWorkbench(wb.id);
            }}
            className="hidden shrink-0 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
            style={{ cursor: 'pointer' }}
          >
            <Archive size={11} />
          </span>
        )}
      </span>
    </button>
  );

  if (!target) return rowButton;
  return (
    <WorkbenchContextMenu
      workbench={target}
      onStartRename={(id) => setRenamingWbId(id)}
      onTogglePin={(id, pinned) => (pinned ? onPinWorkbench!(id) : onUnpinWorkbench!(id))}
      onToggleArchive={(id) => onArchiveWorkbench!(id)}
      onOpenInFinder={(path) => onOpenWorkbenchInFinder!(path)}
      onCopyWorktree={(path) => onCopyWorkbenchPath!(path)}
    >
      {rowButton}
    </WorkbenchContextMenu>
  );
}
