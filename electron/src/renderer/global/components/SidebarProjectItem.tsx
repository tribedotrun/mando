import React, { useState, useCallback, useRef } from 'react';
import { MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { buildUrl } from '#renderer/global/hooks/useApi';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '#renderer/global/components/DropdownMenu';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '#renderer/global/components/Dialog';

interface SidebarProjectItemProps {
  name: string;
  logo?: string | null;
  count: number;
  active: boolean;
  onSelect: () => void;
  onRename: (oldName: string, newName: string) => Promise<void>;
  onRemove: (name: string) => Promise<void>;
}

export function SidebarProjectItem({
  name,
  logo,
  count,
  active,
  onSelect,
  onRename,
  onRemove,
}: SidebarProjectItemProps): React.ReactElement {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [pending, setPending] = useState(false);
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
      className="sidebar-project-item"
      style={{ position: 'relative' }}
      data-menu-open={menuOpen || undefined}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenuOpen(true);
      }}
    >
      {renaming ? (
        <div
          style={{
            padding: '4px 10px',
            borderRadius: 6,
            background: active ? 'var(--color-surface-2)' : 'transparent',
          }}
        >
          <input
            ref={inputRefCb}
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submitRename();
              if (e.key === 'Escape') cancelRename();
            }}
            onBlur={submitRename}
            className="w-full text-[13px]"
            style={{
              background: 'var(--color-surface-3)',
              color: 'var(--color-text-1)',
              border: '1px solid var(--color-accent)',
              borderRadius: 4,
              padding: '2px 6px',
              outline: 'none',
              fontWeight: 400,
            }}
          />
        </div>
      ) : (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <button
            onClick={onSelect}
            className="flex w-full items-center justify-between text-[13px] transition-colors"
            style={{
              background: active ? 'var(--color-surface-2)' : 'transparent',
              color: active ? 'var(--color-text-1)' : 'var(--color-text-2)',
              fontWeight: active ? 500 : 400,
              padding: '6px 10px',
              borderRadius: 6,
              border: 'none',
              cursor: 'pointer',
            }}
          >
            <span className="flex min-w-0 items-center gap-1.5">
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
            <DropdownMenuTrigger asChild>
              <span
                role="button"
                tabIndex={-1}
                onClick={(e) => {
                  e.stopPropagation();
                }}
                className="sidebar-project-dots shrink-0 items-center justify-center transition-colors hover:text-text-2"
                style={{
                  width: 20,
                  height: 20,
                  borderRadius: 4,
                  color: 'var(--color-text-3)',
                }}
              >
                <MoreHorizontal size={14} />
              </span>
            </DropdownMenuTrigger>
            <span
              className="sidebar-project-count shrink-0"
              style={{ fontSize: 11, color: 'var(--color-text-3)', marginLeft: 4 }}
            >
              {count}
            </span>
          </button>
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
              destructive
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
                <strong className="text-text-2">
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
            <label className="mb-4 flex cursor-pointer items-center gap-2 text-[13px] text-text-2">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
                style={{ accentColor: 'var(--color-error)' }}
              />
              I understand this cannot be undone
            </label>
          )}

          <div className="flex justify-end gap-2">
            <button
              onClick={() => {
                setConfirmOpen(false);
                setConfirmed(false);
              }}
              disabled={pending}
              className="cursor-pointer rounded-md border border-border bg-transparent px-3.5 py-1.5 text-[13px] text-text-2 disabled:cursor-default disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              onClick={async () => {
                setPending(true);
                try {
                  await onRemove(name);
                  setConfirmOpen(false);
                  setConfirmed(false);
                } finally {
                  setPending(false);
                }
              }}
              disabled={(count > 0 && !confirmed) || pending}
              className="cursor-pointer rounded-md border-none bg-error px-3.5 py-1.5 text-[13px] text-bg disabled:cursor-default disabled:opacity-50"
            >
              {pending
                ? 'Deleting...'
                : count > 0
                  ? `Delete project and ${count} ${count === 1 ? 'task' : 'tasks'}`
                  : 'Remove'}
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
