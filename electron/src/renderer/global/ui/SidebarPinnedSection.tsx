import React, { useState } from 'react';
import { PinOff, Terminal } from 'lucide-react';
import { compactRelativeTime } from '#renderer/global/service/utils';
import { commitWorkbenchRename } from '#renderer/global/service/workbenchHelpers';
import { StatusIndicator } from '#renderer/global/ui/StatusIndicator';
import {
  ArchivableWorkbenchContextMenu,
  WorkbenchContextMenu,
  WorkbenchRenameInput,
} from '#renderer/global/ui/WorkbenchContextMenu';
import { FINALIZED_STATUSES, type PinnedEntry } from '#renderer/global/types';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

interface SidebarPinnedSectionProps {
  items: PinnedEntry[];
}

export function SidebarPinnedSection({
  items,
}: SidebarPinnedSectionProps): React.ReactElement | null {
  const { state, actions } = useSidebar();
  const [renamingWbId, setRenamingWbId] = useState<number | null>(null);
  if (items.length === 0) return null;

  return (
    <div className="flex flex-col gap-0.5">
      {items.map(({ wb, task, project }) => {
        const ts = task
          ? task.last_activity_at || task.created_at
          : wb.lastActivityAt || wb.createdAt;
        const isActive =
          state.activeTerminalCwd === wb.worktree ||
          (task != null && state.activeTaskId === task.id);
        // Archive is gated on task status so a running task can't be hidden mid-flight.
        const canArchive = task == null || FINALIZED_STATUSES.includes(task.status);
        if (renamingWbId === wb.id) {
          return (
            <div key={wb.id} className="rounded-md px-1.5 py-1">
              <WorkbenchRenameInput
                initialValue={wb.title}
                onCommit={(newTitle) =>
                  commitWorkbenchRename(newTitle, wb.title, wb.id, actions.renameWorkbench, () =>
                    setRenamingWbId(null),
                  )
                }
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
                actions.openTask(task.id, task.workbench_id ?? wb.id);
              } else {
                actions.openTerminalSession({ id: wb.id, project, cwd: wb.worktree });
              }
            }}
            className={`group flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-[13px] transition-colors hover:bg-muted ${isActive ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground'}`}
          >
            <span className="relative shrink-0 translate-y-px">
              <span className="group-hover:invisible">
                {task ? (
                  <StatusIndicator status={task.status} />
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
                  actions.unpinWorkbench(wb.id);
                }}
                className="absolute inset-0 hidden items-center justify-center text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
                style={{ cursor: 'pointer' }}
              >
                <PinOff size={12} />
              </span>
            </span>
            <span className="min-w-0 flex-1 truncate">{wb.title}</span>
            {ts && (
              <span className="shrink-0 text-[11px] text-text-3">{compactRelativeTime(ts)}</span>
            )}
          </button>
        );
        const MenuComponent = canArchive ? ArchivableWorkbenchContextMenu : WorkbenchContextMenu;
        return (
          <MenuComponent
            key={wb.id}
            workbench={{
              id: wb.id,
              title: wb.title,
              worktree: wb.worktree,
              pinnedAt: wb.pinnedAt ?? null,
              archivedAt: wb.archivedAt ?? null,
            }}
            onStartRename={(id) => setRenamingWbId(id)}
          >
            {rowButton}
          </MenuComponent>
        );
      })}
    </div>
  );
}
