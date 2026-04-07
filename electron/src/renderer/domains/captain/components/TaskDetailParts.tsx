import React from 'react';
import { AlignLeft, Copy } from 'lucide-react';
import type { TaskItem } from '#renderer/types';
import { MoreIcon } from '#renderer/domains/captain/components/TaskIcons';
import { copyToClipboard } from '#renderer/utils';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '#renderer/global/components/DropdownMenu';

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
      className="rounded-md px-4 py-1 text-[13px] font-medium"
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
      <div className="mb-2 text-label text-text-4">{label}</div>
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
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          aria-label="More info"
          className="flex items-center justify-center rounded-md transition-colors hover:bg-surface-2"
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
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[220px]">
        {item.context && onViewContext && (
          <DropdownMenuItem onSelect={onViewContext}>
            <AlignLeft size={12} color="var(--color-text-3)" />
            View task brief
          </DropdownMenuItem>
        )}
        {entries.map(({ label, value }) => (
          <DropdownMenuItem key={label} onSelect={() => copyToClipboard(value)}>
            <Copy size={12} color="var(--color-text-3)" />
            {label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
