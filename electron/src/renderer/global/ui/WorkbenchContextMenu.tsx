import React from 'react';
import { Archive, ArchiveRestore, Copy, FolderOpen, Pencil, Pin, PinOff } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '#renderer/global/ui/primitives/context-menu';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

export { WorkbenchRenameInput } from '#renderer/global/ui/WorkbenchRenameInput';

export interface WorkbenchMenuTarget {
  id: number;
  title: string;
  worktree: string;
  pinnedAt?: string | null;
  archivedAt?: string | null;
}

interface WorkbenchContextMenuProps {
  workbench: WorkbenchMenuTarget;
  onStartRename: (id: number) => void;
  children: React.ReactNode;
}

interface WorkbenchContextMenuFrameProps extends WorkbenchContextMenuProps {
  extraActions?: React.ReactNode;
}

function RenameWorkbenchMenuItem({
  workbenchId,
  onStartRename,
}: {
  workbenchId: number;
  onStartRename: (id: number) => void;
}): React.ReactElement {
  return (
    <ContextMenuItem onSelect={() => onStartRename(workbenchId)}>
      <Pencil size={14} />
      Rename
    </ContextMenuItem>
  );
}

function ArchiveWorkbenchMenuItem({ workbenchId }: { workbenchId: number }): React.ReactElement {
  const { actions } = useSidebar();
  return (
    <ContextMenuItem onSelect={() => actions.archiveWorkbench(workbenchId)}>
      <Archive size={14} />
      Archive
    </ContextMenuItem>
  );
}

function UnarchiveWorkbenchMenuItem({ workbenchId }: { workbenchId: number }): React.ReactElement {
  const { actions } = useSidebar();
  return (
    <ContextMenuItem onSelect={() => actions.unarchiveWorkbench(workbenchId)}>
      <ArchiveRestore size={14} />
      Unarchive
    </ContextMenuItem>
  );
}

function WorkbenchContextMenuFrame({
  workbench,
  onStartRename,
  children,
  extraActions,
}: WorkbenchContextMenuFrameProps): React.ReactElement {
  const { actions } = useSidebar();
  const isPinned = !!workbench.pinnedAt;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="min-w-[200px]">
        <ContextMenuItem
          onSelect={() =>
            isPinned ? actions.unpinWorkbench(workbench.id) : actions.pinWorkbench(workbench.id)
          }
        >
          {isPinned ? <PinOff size={14} /> : <Pin size={14} />}
          {isPinned ? 'Unpin' : 'Pin'}
        </ContextMenuItem>
        <RenameWorkbenchMenuItem workbenchId={workbench.id} onStartRename={onStartRename} />
        {extraActions}
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => actions.openWorkbenchInFinder(workbench.worktree)}>
          <FolderOpen size={14} />
          Open in Finder
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.copyWorkbenchPath(workbench.worktree)}>
          <Copy size={14} />
          Copy working directory
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

export function WorkbenchContextMenu(props: WorkbenchContextMenuProps): React.ReactElement {
  return <WorkbenchContextMenuFrame {...props} />;
}

export function ArchivableWorkbenchContextMenu(
  props: WorkbenchContextMenuProps,
): React.ReactElement {
  return (
    <WorkbenchContextMenuFrame
      {...props}
      extraActions={<ArchiveWorkbenchMenuItem workbenchId={props.workbench.id} />}
    />
  );
}

export function UnarchivableWorkbenchContextMenu(
  props: WorkbenchContextMenuProps,
): React.ReactElement {
  return (
    <WorkbenchContextMenuFrame
      {...props}
      extraActions={<UnarchiveWorkbenchMenuItem workbenchId={props.workbench.id} />}
    />
  );
}
