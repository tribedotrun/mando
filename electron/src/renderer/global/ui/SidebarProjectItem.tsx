import React, { useState, useCallback, useRef } from 'react';
import { MoreHorizontal, Pencil, Trash2, SquarePen, ChevronRight } from 'lucide-react';
import { projectLogoUrl } from '#renderer/global/runtime/useApi';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '#renderer/global/ui/dropdown-menu';
import { Button } from '#renderer/global/ui/button';
import { Input } from '#renderer/global/ui/input';
import { DeleteProjectDialog } from '#renderer/global/ui/DeleteProjectDialog';
import type { SidebarChild } from '#renderer/global/service/utils';
import { WorkbenchRow } from '#renderer/global/ui/SidebarChildRows';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

export type { SidebarChild } from '#renderer/global/service/utils';

interface SidebarProjectItemProps {
  name: string;
  logo?: string | null;
  count: number;
  items?: SidebarChild[];
}

export function SidebarProjectItem({
  name,
  logo,
  count,
  items = [],
}: SidebarProjectItemProps): React.ReactElement {
  const { state, actions } = useSidebar();
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const hasActiveWt = items.some((c) => c.wb.worktree === state.activeTerminalCwd) ?? false;
  const [expanded, setExpanded] = useState(hasActiveWt);
  // Auto-expand when this project gains the active workbench.
  if (hasActiveWt && !expanded) setExpanded(true);
  const [renameValue, setRenameValue] = useState(name);
  const submittedRef = useRef(false);
  const [renamingWbId, setRenamingWbId] = useState<number | null>(null);

  const inputRefCb = useCallback((el: HTMLInputElement | null) => {
    if (el) {
      el.focus();
      el.select();
    }
  }, []);

  const submitRename = async () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    setRenaming(false);
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== name) {
      await actions.renameProject(name, trimmed);
    }
  };

  const cancelRename = () => {
    submittedRef.current = true;
    setRenaming(false);
    setRenameValue(name);
  };

  return (
    <div
      className="sidebar-project-item relative min-w-0 overflow-hidden"
      data-menu-open={menuOpen || undefined}
    >
      {renaming ? (
        <div className="rounded-md px-1.5 py-1">
          <Input
            ref={inputRefCb}
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void submitRename();
              if (e.key === 'Escape') cancelRename();
            }}
            onBlur={() => void submitRename()}
            className="h-7 w-full rounded border-ring bg-secondary px-1.5 text-[13px] font-normal"
          />
        </div>
      ) : (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <Button
            variant="ghost"
            onClick={() => {
              setExpanded((v) => !v);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenuOpen(true);
            }}
            className="flex h-auto w-full items-center justify-between rounded-md px-1.5 py-1.5 text-[13px] font-normal text-muted-foreground transition-colors"
          >
            <span className="flex min-w-0 items-center gap-1.5">
              <ChevronRight
                size={10}
                className={`shrink-0 transition-transform duration-150 ${expanded ? 'rotate-90' : ''}`}
              />
              {logo && (
                <img
                  key={logo}
                  src={projectLogoUrl(logo)}
                  alt=""
                  width={16}
                  height={16}
                  className="shrink-0 rounded-sm object-contain"
                  onError={(e) => {
                    (e.target as HTMLImageElement).style.display = 'none';
                  }}
                />
              )}
              <span className="truncate">{name}</span>
            </span>
            <span className="sidebar-project-dots flex shrink-0 items-center gap-1">
              <DropdownMenuTrigger asChild>
                <span
                  role="button"
                  tabIndex={-1}
                  onClick={(e) => {
                    e.stopPropagation();
                  }}
                  className="flex size-5 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
                >
                  <MoreHorizontal size={14} />
                </span>
              </DropdownMenuTrigger>
              <span
                role="button"
                tabIndex={-1}
                onClick={(e) => {
                  e.stopPropagation();
                  actions.newTerminal(name);
                }}
                title="New terminal"
                className="flex size-5 items-center justify-center rounded text-text-3 transition-colors hover:bg-muted-foreground/10 hover:text-text-2"
                style={{ cursor: 'pointer' }}
              >
                <SquarePen size={14} />
              </span>
            </span>
          </Button>
          <DropdownMenuContent align="end" className="min-w-[130px]">
            <DropdownMenuItem
              onSelect={() => {
                submittedRef.current = false;
                setRenaming(true);
                setRenameValue(name);
              }}
            >
              <Pencil size={12} />
              Rename
            </DropdownMenuItem>
            <DropdownMenuItem
              variant="destructive"
              onSelect={() => {
                setConfirmOpen(true);
              }}
            >
              <Trash2 size={12} />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      <DeleteProjectDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        name={name}
        count={count}
      />

      {/* Expanded children: workbench-first rows, sorted by last activity. */}
      {expanded && items.length > 0 && (
        <div className="flex flex-col gap-0.5 pb-1 pt-0.5">
          {items.map((child) => (
            <WorkbenchRow
              key={`wb:${child.wb.id}:${child.task?.id ?? 'none'}`}
              projectName={name}
              wb={child.wb}
              task={child.task}
              renamingWbId={renamingWbId}
              setRenamingWbId={setRenamingWbId}
            />
          ))}
        </div>
      )}
    </div>
  );
}
