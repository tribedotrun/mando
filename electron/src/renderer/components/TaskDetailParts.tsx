import React, { useState } from 'react';
import type { TaskItem } from '#renderer/types';

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

export function DetailOverflowMenu({ item }: { item: TaskItem }): React.ReactElement {
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
        className="flex items-center justify-center rounded"
        style={{
          width: 28,
          height: 28,
          background: 'transparent',
          color: 'var(--color-text-2)',
          border: '1px solid var(--color-border)',
          cursor: 'pointer',
          fontSize: 14,
          borderRadius: 6,
        }}
      >
        &hellip;
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
          {entries.map(({ label, value }) => (
            <button
              key={label}
              onClick={() => copyAndClose(value)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] hover:bg-[var(--color-surface-2)]"
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

export function ContextToggle({ context }: { context: string }): React.ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-5">
      <button
        onClick={() => setOpen((v) => !v)}
        className="mb-2 flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-widest"
        style={{
          color: 'var(--color-text-4)',
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          padding: 0,
        }}
      >
        <svg
          width="8"
          height="8"
          viewBox="0 0 8 8"
          fill="currentColor"
          style={{
            transition: 'transform 150ms',
            transform: open ? 'rotate(90deg)' : 'none',
          }}
        >
          <path d="M2 1l4 3-4 3V1z" />
        </svg>
        Context
      </button>
      {open && (
        <p className="text-[11px] leading-relaxed" style={{ color: 'var(--color-text-3)' }}>
          {context}
        </p>
      )}
    </div>
  );
}
