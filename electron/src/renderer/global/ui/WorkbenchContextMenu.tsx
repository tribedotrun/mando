import React, { useCallback, useRef, useState } from 'react';
import { Archive, Copy, FolderOpen, Pencil, Pin, PinOff } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '#renderer/global/ui/context-menu';
import { Input } from '#renderer/global/ui/input';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

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
      <ContextMenuTrigger
        asChild
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
      >
        {children}
      </ContextMenuTrigger>
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

interface RenameInputProps {
  initialValue: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
  className?: string;
}

export function WorkbenchRenameInput({
  initialValue,
  onCommit,
  onCancel,
  className,
}: RenameInputProps): React.ReactElement {
  const [value, setValue] = useState(initialValue);
  const submittedRef = useRef(false);
  const inputRefCb = useCallback((element: HTMLInputElement | null) => {
    if (element) {
      element.focus();
      element.select();
    }
  }, []);
  const commit = () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    onCommit(value);
  };
  const cancel = () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    onCancel();
  };
  return (
    <Input
      ref={inputRefCb}
      value={value}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commit();
        if (event.key === 'Escape') cancel();
      }}
      onBlur={commit}
      className={
        className ?? 'h-6 w-full rounded border-ring bg-secondary px-2 text-[12px] font-normal'
      }
    />
  );
}
