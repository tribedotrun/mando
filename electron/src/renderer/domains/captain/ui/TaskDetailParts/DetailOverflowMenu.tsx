import React from 'react';
import { AlignLeft, Copy } from 'lucide-react';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/global/types';
import { MoreIcon } from '#renderer/domains/captain/ui/TaskIcons';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { planCopyLabel } from '#renderer/domains/captain/service/projectHelpers';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '#renderer/global/ui/primitives/dropdown-menu';
import { Button } from '#renderer/global/ui/primitives/button';

export function DetailOverflowMenu({
  item,
  onViewContext,
  onCancel,
}: {
  item: TaskItem;
  onViewContext?: () => void;
  onCancel?: () => void;
}): React.ReactElement {
  const entries: { label: string; value: string }[] = [];
  if (item.branch) entries.push({ label: 'Copy branch', value: item.branch });
  if (item.worktree) entries.push({ label: 'Copy working directory', value: item.worktree });
  if (item.plan) {
    entries.push({ label: planCopyLabel(item.plan), value: item.plan });
  }

  const showCancel = !!onCancel && !FINALIZED_STATUSES.includes(item.status);
  const showViewBrief = !!(item.context && onViewContext);
  const showInfoEntries = entries.length > 0;
  const hasInfo = showViewBrief || showInfoEntries;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-xs" aria-label="More info">
          <MoreIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[220px]">
        {showCancel && (
          <DropdownMenuItem variant="destructive" onSelect={onCancel}>
            Cancel task
          </DropdownMenuItem>
        )}
        {showCancel && hasInfo && <DropdownMenuSeparator />}
        {showViewBrief && (
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
        {!showCancel && !hasInfo && (
          <DropdownMenuItem disabled>
            <span className="text-text-4">No actions available</span>
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
