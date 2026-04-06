import React, { useState, useCallback, useRef } from 'react';

interface SidebarProjectItemProps {
  name: string;
  count: number;
  active: boolean;
  onSelect: () => void;
  onRename: (oldName: string, newName: string) => Promise<void>;
  onRemove: (name: string) => Promise<void>;
}

export function SidebarProjectItem({
  name,
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

  const closeMenu = () => {
    setMenuOpen(false);
  };

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
          <span className="truncate">{name}</span>
          <span
            role="button"
            tabIndex={-1}
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(!menuOpen);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.stopPropagation();
                setMenuOpen(!menuOpen);
              }
            }}
            className="sidebar-project-dots shrink-0 items-center justify-center transition-colors hover:text-[var(--color-text-2)]"
            style={{
              width: 20,
              height: 20,
              borderRadius: 4,
              color: 'var(--color-text-3)',
            }}
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <circle cx="3.5" cy="8" r="1.5" />
              <circle cx="8" cy="8" r="1.5" />
              <circle cx="12.5" cy="8" r="1.5" />
            </svg>
          </span>
          <span
            className="sidebar-project-count shrink-0"
            style={{ fontSize: 11, color: 'var(--color-text-3)', marginLeft: 4 }}
          >
            {count}
          </span>
        </button>
      )}

      {/* Dropdown menu */}
      {menuOpen && (
        <>
          <div style={{ position: 'fixed', inset: 0, zIndex: 49 }} onMouseDown={closeMenu} />
          <div
            style={{
              position: 'absolute',
              top: '100%',
              right: 0,
              marginTop: 2,
              background: 'var(--color-surface-2)',
              border: '1px solid var(--color-border)',
              borderRadius: 8,
              padding: '4px 0',
              minWidth: 130,
              zIndex: 50,
              boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
            }}
          >
            <button
              onClick={() => {
                closeMenu();
                submittedRef.current = false;
                setRenaming(true);
                setRenameValue(name);
              }}
              className="flex w-full items-center gap-2 text-[12px] transition-colors hover:bg-[var(--color-surface-3)]"
              style={{
                padding: '6px 12px',
                border: 'none',
                cursor: 'pointer',
                background: 'transparent',
                color: 'var(--color-text-2)',
              }}
            >
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                <path
                  d="M11.5 2.5l2 2L5 13H3v-2l8.5-8.5z"
                  strokeWidth="1.5"
                  strokeLinejoin="round"
                />
              </svg>
              Rename
            </button>
            <button
              onClick={() => {
                closeMenu();
                setConfirmed(false);
                setConfirmOpen(true);
              }}
              className="flex w-full items-center gap-2 text-[12px] transition-colors hover:bg-[var(--color-surface-3)]"
              style={{
                padding: '6px 12px',
                border: 'none',
                cursor: 'pointer',
                background: 'transparent',
                color: 'var(--color-error)',
              }}
            >
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor">
                <path d="M3 4h10M6 4V3h4v1M5 4v9h6V4" strokeWidth="1.5" strokeLinejoin="round" />
              </svg>
              Remove
            </button>
          </div>
        </>
      )}

      {/* Remove confirmation dialog */}
      {confirmOpen && (
        <>
          <div
            style={{ position: 'fixed', inset: 0, zIndex: 99, background: 'var(--color-overlay)' }}
            onMouseDown={() => {
              if (!pending) {
                setConfirmOpen(false);
                setConfirmed(false);
              }
            }}
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-label={count > 0 ? 'Delete project and tasks' : 'Remove project'}
            style={{
              position: 'fixed',
              top: '50%',
              left: '50%',
              transform: 'translate(-50%, -50%)',
              background: 'var(--color-surface-2)',
              border: '1px solid var(--color-border)',
              borderRadius: 10,
              padding: '20px 24px',
              zIndex: 100,
              boxShadow: '0 16px 48px rgba(0,0,0,0.5)',
              minWidth: 320,
              maxWidth: 400,
            }}
          >
            <div
              style={{
                fontSize: 14,
                color: 'var(--color-text-1)',
                marginBottom: 4,
                fontWeight: 500,
              }}
            >
              {count > 0 ? 'Delete project and tasks?' : 'Remove project?'}
            </div>
            <div
              style={{
                fontSize: 13,
                color: 'var(--color-text-3)',
                marginBottom: count > 0 ? 12 : 16,
                lineHeight: 1.45,
              }}
            >
              {count > 0 ? (
                <>
                  &ldquo;{name}&rdquo; and{' '}
                  <strong style={{ color: 'var(--color-text-2)' }}>
                    {count} {count === 1 ? 'task' : 'tasks'}
                  </strong>{' '}
                  belonging to it will be permanently deleted. Project files on disk are not
                  affected.
                </>
              ) : (
                <>
                  &ldquo;{name}&rdquo; will be removed from Mando. Project files on disk are not
                  affected.
                </>
              )}
            </div>

            {count > 0 && (
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  fontSize: 13,
                  color: 'var(--color-text-2)',
                  marginBottom: 16,
                  cursor: 'pointer',
                }}
              >
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                  style={{ accentColor: 'var(--color-error)' }}
                />
                I understand this cannot be undone
              </label>
            )}

            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button
                onClick={() => {
                  setConfirmOpen(false);
                  setConfirmed(false);
                }}
                disabled={pending}
                style={{
                  padding: '6px 14px',
                  fontSize: 13,
                  borderRadius: 6,
                  border: '1px solid var(--color-border)',
                  background: 'transparent',
                  color: 'var(--color-text-2)',
                  cursor: pending ? 'default' : 'pointer',
                  opacity: pending ? 0.5 : 1,
                }}
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
                style={{
                  padding: '6px 14px',
                  fontSize: 13,
                  borderRadius: 6,
                  border: 'none',
                  background: 'var(--color-error)',
                  color: 'var(--color-bg)',
                  cursor: (count > 0 && !confirmed) || pending ? 'default' : 'pointer',
                  opacity: (count > 0 && !confirmed) || pending ? 0.5 : 1,
                }}
              >
                {pending
                  ? 'Deleting...'
                  : count > 0
                    ? `Delete project and ${count} ${count === 1 ? 'task' : 'tasks'}`
                    : 'Remove'}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
