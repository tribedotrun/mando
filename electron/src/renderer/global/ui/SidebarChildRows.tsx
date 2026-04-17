import React from 'react';
import { Archive, Terminal, Pin } from 'lucide-react';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/global/types';
import { compactRelativeTime } from '#renderer/global/service/utils';
import { commitWorkbenchRename } from '#renderer/global/service/workbenchHelpers';
import { StatusIcon } from '#renderer/global/ui/StatusIndicator';
import {
  ArchivableWorkbenchContextMenu,
  WorkbenchContextMenu,
  WorkbenchRenameInput,
  type WorkbenchMenuTarget,
} from '#renderer/global/ui/WorkbenchContextMenu';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

export function WorkbenchRow({
  projectName,
  wb,
  task,
  renamingWbId,
  setRenamingWbId,
}: {
  projectName: string;
  wb: import('#renderer/global/types').WorkbenchItem;
  task?: TaskItem;
  renamingWbId: number | null;
  setRenamingWbId: (id: number | null) => void;
}): React.ReactElement {
  const { state, actions } = useSidebar();
  const isActive =
    state.activeTerminalCwd === wb.worktree || (task != null && state.activeTaskId === task.id);
  // wb.id <= 0 is a synthetic placeholder for a task with no backing workbench --
  // pin/archive/rename all target real workbench IDs, so suppress them here.
  const hasRealWorkbench = wb.id > 0;
  const canPin = hasRealWorkbench;
  const canArchive = hasRealWorkbench && (task ? FINALIZED_STATUSES.includes(task.status) : true);
  const activity = task
    ? task.last_activity_at || task.created_at
    : wb.lastActivityAt || wb.createdAt;
  const target: WorkbenchMenuTarget = {
    id: wb.id,
    title: wb.title,
    worktree: wb.worktree,
    pinnedAt: wb.pinnedAt ?? null,
    archivedAt: wb.archivedAt ?? null,
  };

  if (renamingWbId === wb.id) {
    return (
      <WorkbenchRenameInput
        initialValue={wb.title}
        onCommit={(newTitle) =>
          commitWorkbenchRename(newTitle, wb.title, wb.id, actions.renameWorkbench, () =>
            setRenamingWbId(null),
          )
        }
        onCancel={() => setRenamingWbId(null)}
      />
    );
  }

  const handleClick = () => {
    if (task) {
      actions.openTask(task.id, wb.id || undefined);
    } else {
      actions.openTerminalSession({ id: wb.id, project: projectName, cwd: wb.worktree });
    }
  };

  const rowButton = (
    <button
      onClick={handleClick}
      className={`sidebar-workbench-item group flex w-full select-none items-center gap-2 rounded px-2 py-1 text-left text-caption transition-colors hover:bg-muted hover:text-foreground ${isActive ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground'}`}
      style={{
        background: isActive ? undefined : 'none',
        border: 'none',
        cursor: 'pointer',
      }}
    >
      <span className="relative inline-flex w-4 shrink-0 items-center justify-center translate-y-px">
        {task ? (
          <span className={canPin ? 'group-hover:invisible' : ''}>
            <StatusIcon status={task.status} />
          </span>
        ) : (
          <Terminal size={11} className={`text-text-3 ${canPin ? 'group-hover:invisible' : ''}`} />
        )}
        {canPin && (
          <span
            role="button"
            tabIndex={-1}
            title="Pin workbench"
            onClick={(e) => {
              e.stopPropagation();
              actions.pinWorkbench(wb.id);
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
          <span className={`text-[11px] text-text-3 ${canArchive ? 'group-hover:hidden' : ''}`}>
            {compactRelativeTime(activity)}
          </span>
        )}
        {canArchive && (
          <span
            role="button"
            tabIndex={-1}
            title="Archive workbench"
            onClick={(e) => {
              e.stopPropagation();
              actions.archiveWorkbench(wb.id);
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

  if (!hasRealWorkbench) return rowButton;

  const MenuComponent = canArchive ? ArchivableWorkbenchContextMenu : WorkbenchContextMenu;

  return (
    <MenuComponent workbench={target} onStartRename={(id) => setRenamingWbId(id)}>
      {rowButton}
    </MenuComponent>
  );
}
