import React, { useState, useCallback, useRef } from 'react';
import {
  MoreHorizontal,
  Pencil,
  Trash2,
  SquarePen,
  ChevronRight,
  Archive,
  Terminal,
  Pin,
} from 'lucide-react';
import { buildUrl } from '#renderer/global/hooks/useApi';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '#renderer/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '#renderer/components/ui/dialog';
import { Button } from '#renderer/components/ui/button';
import { Input } from '#renderer/components/ui/input';
import { Checkbox } from '#renderer/components/ui/checkbox';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/types';
import { compactRelativeTime } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';

interface SidebarWorktree {
  id?: number;
  cwd: string;
  name: string;
}

interface SidebarProjectItemProps {
  name: string;
  logo?: string | null;
  count: number;
  onRename: (oldName: string, newName: string) => Promise<void>;
  onRemove: (name: string) => Promise<void>;
  onNewTerminal?: (project: string) => void;
  tasks?: TaskItem[];
  onOpenTask?: (taskId: number) => void;
  worktrees?: SidebarWorktree[];
  activeWorktreeCwd?: string | null;
  onOpenWorktree?: (worktree: { project: string; cwd: string }) => void;
  onArchiveWorkbench?: (id: number) => void;
  onPinWorkbench?: (id: number) => void;
}

export function SidebarProjectItem({
  name,
  logo,
  count,
  onRename,
  onRemove,
  onNewTerminal,
  tasks = [],
  onOpenTask,
  worktrees = [],
  activeWorktreeCwd,
  onOpenWorktree,
  onArchiveWorkbench,
  onPinWorkbench,
}: SidebarProjectItemProps): React.ReactElement {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [pending, setPending] = useState(false);
  const hasActiveWt = worktrees?.some((wt) => wt.cwd === activeWorktreeCwd) ?? false;
  const [expanded, setExpanded] = useState(hasActiveWt);
  // Auto-expand when this project gains the active workbench.
  if (hasActiveWt && !expanded) setExpanded(true);
  const [renameValue, setRenameValue] = useState(name);
  const submittedRef = useRef(false);

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
      await onRename(name, trimmed);
    }
  };

  const cancelRename = () => {
    submittedRef.current = true;
    setRenaming(false);
    setRenameValue(name);
  };

  const dialogTitle = count > 0 ? 'Delete project and tasks?' : 'Remove project?';

  return (
    <div
      className="sidebar-project-item relative min-w-0 overflow-hidden"
      data-menu-open={menuOpen || undefined}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenuOpen(true);
      }}
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
                  src={buildUrl(`/api/images/${logo}`)}
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
              {onNewTerminal && (
                <span
                  role="button"
                  tabIndex={-1}
                  onClick={(e) => {
                    e.stopPropagation();
                    onNewTerminal(name);
                  }}
                  title="New terminal"
                  className="flex size-5 items-center justify-center rounded text-text-3 transition-colors hover:bg-muted-foreground/10 hover:text-text-2"
                  style={{ cursor: 'pointer' }}
                >
                  <SquarePen size={14} />
                </span>
              )}
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
                setConfirmed(false);
                setConfirmOpen(true);
              }}
            >
              <Trash2 size={12} />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      <Dialog
        open={confirmOpen}
        onOpenChange={(open) => {
          if (!open && !pending) {
            setConfirmOpen(false);
            setConfirmed(false);
          }
        }}
      >
        <DialogContent aria-label={dialogTitle}>
          <DialogTitle>{dialogTitle}</DialogTitle>
          <DialogDescription>
            {count > 0 ? (
              <>
                &ldquo;{name}&rdquo; and{' '}
                <strong className="text-muted-foreground">
                  {count} {count === 1 ? 'task' : 'tasks'}
                </strong>{' '}
                belonging to it will be permanently deleted. Project files on disk are not affected.
              </>
            ) : (
              <>
                &ldquo;{name}&rdquo; will be removed from Mando. Project files on disk are not
                affected.
              </>
            )}
          </DialogDescription>

          {count > 0 && (
            <label className="mb-4 flex cursor-pointer items-center gap-2 text-[13px] text-muted-foreground">
              <Checkbox
                checked={confirmed}
                onCheckedChange={(checked) => setConfirmed(checked === true)}
              />
              I understand this cannot be undone
            </label>
          )}

          <div className="flex justify-end gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setConfirmOpen(false);
                setConfirmed(false);
              }}
              disabled={pending}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                setPending(true);
                void onRemove(name)
                  .then(() => {
                    setConfirmOpen(false);
                    setConfirmed(false);
                  })
                  .catch((err) => console.error('Remove project failed', err))
                  .finally(() => {
                    setPending(false);
                  });
              }}
              disabled={(count > 0 && !confirmed) || pending}
            >
              {pending
                ? 'Deleting...'
                : count > 0
                  ? `Delete project and ${count} ${count === 1 ? 'task' : 'tasks'}`
                  : 'Remove'}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Expanded children: tasks + worktrees */}
      {expanded && (tasks.length > 0 || worktrees.length > 0) && (
        <div className="flex flex-col gap-0.5 pb-1 pt-0.5">
          {tasks.map((task) => {
            const canArchive =
              task.workbench_id != null &&
              onArchiveWorkbench &&
              FINALIZED_STATUSES.includes(task.status);
            const canPin = task.workbench_id != null && onPinWorkbench;
            return (
              <button
                key={task.id}
                onClick={() => onOpenTask?.(task.id)}
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
                <span className="min-w-0 flex-1 truncate">{task.title}</span>
                <span className="flex shrink-0 items-center gap-1">
                  {(task.last_activity_at || task.created_at) && (
                    <span
                      className={`text-[11px] text-text-3 ${canArchive ? 'group-hover:hidden' : ''}`}
                    >
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
          })}
          {worktrees.map((wt) => {
            const isActive = activeWorktreeCwd === wt.cwd;
            const canPinWt = wt.id != null && onPinWorkbench;
            return (
              <button
                key={wt.cwd}
                onClick={() => onOpenWorktree?.({ project: name, cwd: wt.cwd })}
                className={`sidebar-workbench-item group flex w-full items-center gap-2 rounded px-2 py-1 text-left text-[12px] transition-colors hover:bg-muted hover:text-foreground ${isActive ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground'}`}
                style={{
                  background: isActive ? undefined : 'none',
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                <span className="relative inline-flex w-4 shrink-0 items-center justify-center">
                  <Terminal
                    size={11}
                    className={`text-text-3 ${canPinWt ? 'group-hover:invisible' : ''}`}
                  />
                  {canPinWt && (
                    <span
                      role="button"
                      tabIndex={-1}
                      title="Pin workbench"
                      onClick={(e) => {
                        e.stopPropagation();
                        onPinWorkbench!(wt.id!);
                      }}
                      className="absolute inset-0 hidden items-center justify-center text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
                      style={{ cursor: 'pointer' }}
                    >
                      <Pin size={12} />
                    </span>
                  )}
                </span>
                <span className="min-w-0 flex-1 truncate">{wt.name}</span>
                <span className="flex shrink-0 items-center gap-0.5">
                  {wt.id != null && onArchiveWorkbench && (
                    <span
                      role="button"
                      tabIndex={-1}
                      title="Archive workbench"
                      onClick={(e) => {
                        e.stopPropagation();
                        onArchiveWorkbench(wt.id!);
                      }}
                      className="hidden items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
                      style={{ cursor: 'pointer' }}
                    >
                      <Archive size={11} />
                    </span>
                  )}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
