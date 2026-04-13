import React, { useCallback, useRef, useState } from 'react';
import { Pin, PinOff, Pencil, Archive, FolderOpen, Copy } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from '#renderer/components/ui/context-menu';
import { Input } from '#renderer/components/ui/input';

export interface WorkbenchMenuTarget {
  id: number;
  title: string;
  worktree: string;
  pinnedAt?: string | null;
  archivedAt?: string | null;
}

interface Props {
  workbench: WorkbenchMenuTarget;
  onStartRename: (id: number) => void;
  onTogglePin: (id: number, pinned: boolean) => void;
  onToggleArchive: (id: number) => void;
  onOpenInFinder: (worktree: string) => void;
  onCopyWorktree: (worktree: string) => void;
  canRename?: boolean;
  canArchive?: boolean;
  children: React.ReactNode;
}

export function WorkbenchContextMenu({
  workbench,
  onStartRename,
  onTogglePin,
  onToggleArchive,
  onOpenInFinder,
  onCopyWorktree,
  canRename = true,
  canArchive = true,
  children,
}: Props): React.ReactElement {
  const isPinned = !!workbench.pinnedAt;
  return (
    <ContextMenu>
      <ContextMenuTrigger
        asChild
        onContextMenu={(e) => {
          e.stopPropagation();
        }}
      >
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent className="min-w-[200px]">
        <ContextMenuItem onSelect={() => onTogglePin(workbench.id, !isPinned)}>
          {isPinned ? <PinOff size={14} /> : <Pin size={14} />}
          {isPinned ? 'Unpin' : 'Pin'}
        </ContextMenuItem>
        {canRename && (
          <ContextMenuItem onSelect={() => onStartRename(workbench.id)}>
            <Pencil size={14} />
            Rename
          </ContextMenuItem>
        )}
        {canArchive && (
          <ContextMenuItem onSelect={() => onToggleArchive(workbench.id)}>
            <Archive size={14} />
            Archive
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => onOpenInFinder(workbench.worktree)}>
          <FolderOpen size={14} />
          Open in Finder
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => onCopyWorktree(workbench.worktree)}>
          <Copy size={14} />
          Copy working directory
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
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
  const inputRefCb = useCallback((el: HTMLInputElement | null) => {
    if (el) {
      el.focus();
      el.select();
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
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit();
        if (e.key === 'Escape') cancel();
      }}
      onBlur={commit}
      className={
        className ?? 'h-6 w-full rounded border-ring bg-secondary px-2 text-[12px] font-normal'
      }
    />
  );
}
