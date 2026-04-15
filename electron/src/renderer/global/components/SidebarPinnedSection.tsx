import React, { useState } from 'react';
import { PinOff, Terminal } from 'lucide-react';
import { compactRelativeTime } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import {
  WorkbenchContextMenu,
  WorkbenchRenameInput,
} from '#renderer/global/components/WorkbenchContextMenu';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/types';

interface PinnedWorkbench {
  id: number;
  worktree: string;
  title: string;
  createdAt: string;
  lastActivityAt?: string;
  pinnedAt?: string | null;
  archivedAt?: string | null;
}

interface PinnedEntry {
  wb: PinnedWorkbench;
  task?: TaskItem;
  project: string;
}

interface SidebarPinnedSectionProps {
  items: PinnedEntry[];
  activeTerminalCwd?: string | null;
  activeTaskId?: number | null;
  onOpenTask?: (taskId: number, workbenchId?: number) => void;
  onOpenTerminalSession?: (worktree: { id?: number; project: string; cwd: string }) => void;
  onUnpin: (id: number) => void;
  onPin?: (id: number) => void;
  onArchiveWorkbench?: (id: number) => void;
  onRenameWorkbench?: (id: number, title: string) => void;
  onOpenWorkbenchInFinder?: (worktree: string) => void;
  onCopyWorkbenchPath?: (worktree: string) => void;
}

export function SidebarPinnedSection({
  items,
  activeTerminalCwd,
  activeTaskId,
  onOpenTask,
  onOpenTerminalSession,
  onUnpin,
  onPin,
  onArchiveWorkbench,
  onRenameWorkbench,
  onOpenWorkbenchInFinder,
  onCopyWorkbenchPath,
}: SidebarPinnedSectionProps): React.ReactElement | null {
  const [renamingWbId, setRenamingWbId] = useState<number | null>(null);
  if (items.length === 0) return null;

  return (
    <div className="flex flex-col gap-0.5">
      {items.map(({ wb, task, project }) => {
        const label = task ? task.title || task.original_prompt || 'Untitled task' : wb.title;
        const ts = task
          ? task.last_activity_at || task.created_at
          : wb.lastActivityAt || wb.createdAt;
        const isActive =
          activeTerminalCwd === wb.worktree || (task != null && activeTaskId === task.id);
        const canContextMenu =
          onRenameWorkbench &&
          onArchiveWorkbench &&
          onPin &&
          onOpenWorkbenchInFinder &&
          onCopyWorkbenchPath;
        // Task-backed rows display task.title, but Rename mutates wb.title -- hide
        // Rename so the user never triggers a no-op. Archive mirrors the
        // FINALIZED_STATUSES gate used for task rows in SidebarProjectItem so a
        // running task can't be hidden mid-flight.
        const canRename = task == null;
        const canArchive = task == null || FINALIZED_STATUSES.includes(task.status);
        if (renamingWbId === wb.id) {
          return (
            <div key={wb.id} className="rounded-md px-1.5 py-1">
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
                className="h-7 w-full rounded border-ring bg-secondary px-1.5 text-[13px] font-normal"
              />
            </div>
          );
        }
        const rowButton = (
          <button
            key={wb.id}
            onClick={() => {
              if (task) {
                onOpenTask?.(task.id, task.workbench_id ?? wb.id);
              } else {
                onOpenTerminalSession?.({ id: wb.id, project, cwd: wb.worktree });
              }
            }}
            className={`group flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-[13px] transition-colors hover:bg-muted ${isActive ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground'}`}
          >
            <span className="relative shrink-0 translate-y-px">
              <span className="group-hover:invisible">
                {task ? (
                  <StatusIcon status={task.status} />
                ) : (
                  <span className="inline-flex w-4 items-center justify-center">
                    <Terminal size={14} className="text-text-3" />
                  </span>
                )}
              </span>
              <span
                role="button"
                tabIndex={-1}
                title="Unpin"
                onClick={(e) => {
                  e.stopPropagation();
                  onUnpin(wb.id);
                }}
                className="absolute inset-0 hidden items-center justify-center text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
                style={{ cursor: 'pointer' }}
              >
                <PinOff size={12} />
              </span>
            </span>
            <span className="min-w-0 flex-1 truncate">{label}</span>
            {ts && (
              <span className="shrink-0 text-[11px] text-text-3">{compactRelativeTime(ts)}</span>
            )}
          </button>
        );
        if (!canContextMenu) return rowButton;
        return (
          <WorkbenchContextMenu
            key={wb.id}
            workbench={{
              id: wb.id,
              title: wb.title,
              worktree: wb.worktree,
              pinnedAt: wb.pinnedAt ?? null,
              archivedAt: wb.archivedAt ?? null,
            }}
            onStartRename={(id) => setRenamingWbId(id)}
            onTogglePin={(id, pinned) => (pinned ? onPin!(id) : onUnpin(id))}
            onToggleArchive={(id) => onArchiveWorkbench!(id)}
            onOpenInFinder={(path) => onOpenWorkbenchInFinder!(path)}
            onCopyWorktree={(path) => onCopyWorkbenchPath!(path)}
            canRename={canRename}
            canArchive={canArchive}
          >
            {rowButton}
          </WorkbenchContextMenu>
        );
      })}
    </div>
  );
}
