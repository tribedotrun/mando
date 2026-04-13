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
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';

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
    <Button variant={accent ? 'default' : 'outline'} size="sm" onClick={onClick}>
      {label}
    </Button>
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
        <Button variant="ghost" size="icon-xs" aria-label="More info">
          <MoreIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[220px]">
        {item.context && onViewContext && (
          <DropdownMenuItem onSelect={onViewContext}>
            <AlignLeft size={12} color="var(--text-3)" />
            View task brief
          </DropdownMenuItem>
        )}
        {entries.map(({ label, value }) => (
          <DropdownMenuItem key={label} onSelect={() => void copyToClipboard(value)}>
            <Copy size={12} color="var(--text-3)" />
            {label}
          </DropdownMenuItem>
        ))}
        {entries.length === 0 && !(item.context && onViewContext) && (
          <DropdownMenuItem disabled>
            <span className="text-text-4">No actions available</span>
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
