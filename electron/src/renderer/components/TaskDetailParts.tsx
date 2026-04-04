import React, { useState } from 'react';
import type { TaskItem } from '#renderer/types';
import { MoreIcon } from '#renderer/components/TaskIcons';

export function ActionButton({
  label,
  onClick,
  accent,
}: {
  label: string;
  onClick: () => void;
  accent?: boolean;
}): React.ReactElement {
  return (
    <button
      onClick={onClick}
      className="rounded-md px-4 py-1.5 text-[13px] font-medium"
      style={{
        background: accent ? 'var(--color-accent)' : 'transparent',
        color: accent ? 'var(--color-bg)' : 'var(--color-text-2)',
        border: accent ? 'none' : '1px solid var(--color-border)',
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );
}

export function DetailSection({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mb-5">
      <div
        className="mb-2 text-[10px] font-medium uppercase tracking-widest"
        style={{ color: 'var(--color-text-4)' }}
      >
        {label}
      </div>
      {children}
    </div>
  );
}

export function DetailOverflowMenu({
  item,
  onViewContext,
}: {
  item: TaskItem;
  onViewContext?: () => void;
}): React.ReactElement {
  const [open, setOpen] = useState(false);

  const copyAndClose = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
    setOpen(false);
  };

  const entries: { label: string; value: string }[] = [];
  if (item.branch) entries.push({ label: 'Copy branch', value: item.branch });
  if (item.worktree) entries.push({ label: 'Copy working directory', value: item.worktree });
  if (item.plan) {
    const planLabel = item.plan.endsWith('adopt-handoff.md')
      ? 'Copy handoff path'
      : 'Copy brief path';
    entries.push({ label: planLabel, value: item.plan });
  }

  return (
    <div
      className="relative"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <button
        onClick={() => setOpen((v) => !v)}
        aria-label="More info"
        className="flex items-center justify-center rounded-md transition-colors hover:bg-[var(--color-surface-2)]"
        style={{
          width: 28,
          height: 28,
          background: 'transparent',
          color: 'var(--color-text-3)',
          border: 'none',
          cursor: 'pointer',
        }}
      >
        <MoreIcon />
      </button>
      {open && (
        <div
          className="absolute right-0 top-full z-50 mt-1 min-w-[220px] rounded-lg py-1"
          style={{
            background: 'var(--color-surface-3)',
            border: '1px solid var(--color-border)',
            boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
          }}
        >
          {item.context && onViewContext && (
            <button
              onClick={() => {
                setOpen(false);
                onViewContext();
              }}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-caption hover:bg-[var(--color-surface-2)]"
              style={{
                color: 'var(--color-text-1)',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                stroke="var(--color-text-3)"
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M1.5 3h9M1.5 6h9M1.5 9h5" />
              </svg>
              View task brief
            </button>
          )}
          {entries.map(({ label, value }) => (
            <button
              key={label}
              onClick={() => copyAndClose(value)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-caption hover:bg-[var(--color-surface-2)]"
              style={{
                color: 'var(--color-text-1)',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                stroke="var(--color-text-3)"
                strokeWidth="1.2"
                strokeLinecap="round"
              >
                <rect x="4" y="4" width="7" height="7" rx="1" />
                <path d="M8 4V2.5A1.5 1.5 0 006.5 1H2.5A1.5 1.5 0 001 2.5v4A1.5 1.5 0 002.5 8H4" />
              </svg>
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
