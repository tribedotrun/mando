import React from 'react';
import { PinOff, Terminal } from 'lucide-react';
import { compactRelativeTime } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import type { TaskItem } from '#renderer/types';

interface PinnedWorkbench {
  id: number;
  worktree: string;
  title: string;
  createdAt: string;
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
  onOpenTask?: (taskId: number) => void;
  onOpenTerminalSession?: (worktree: { project: string; cwd: string }) => void;
  onUnpin: (id: number) => void;
}

export function SidebarPinnedSection({
  items,
  activeTerminalCwd,
  activeTaskId,
  onOpenTask,
  onOpenTerminalSession,
  onUnpin,
}: SidebarPinnedSectionProps): React.ReactElement | null {
  if (items.length === 0) return null;

  return (
    <div className="flex flex-col gap-0.5">
      {items.map(({ wb, task, project }) => {
        const label = task ? task.title : wb.title;
        const ts = task ? task.last_activity_at || task.created_at : wb.createdAt;
        const isActive =
          activeTerminalCwd === wb.worktree || (task != null && activeTaskId === task.id);
        return (
          <button
            key={wb.id}
            onClick={() => {
              if (task) {
                onOpenTask?.(task.id);
              } else {
                onOpenTerminalSession?.({ project, cwd: wb.worktree });
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
      })}
    </div>
  );
}
