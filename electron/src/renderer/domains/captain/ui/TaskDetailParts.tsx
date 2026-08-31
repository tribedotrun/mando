import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import { AlignLeft, Copy, Presentation } from 'lucide-react';
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
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { evidenceDeckPath } from '#renderer/domains/captain/service/evidenceHelpers';

export function DetailOverflowMenu({
  item,
  onViewContext,
  onResumeRateLimited,
  resumeRateLimitedPending = false,
  onCancel,
}: {
  item: TaskItem;
  onViewContext?: () => void;
  onResumeRateLimited?: () => void;
  resumeRateLimitedPending?: boolean;
  onCancel?: () => void;
}): React.ReactElement {
  const entries: { label: string; value: string }[] = [];
  if (item.branch) entries.push({ label: 'Copy branch', value: item.branch });
  if (item.worktree) entries.push({ label: 'Copy working directory', value: item.worktree });
  if (item.plan) {
    entries.push({ label: planCopyLabel(item.plan), value: item.plan });
  }

  const showCancel = !!onCancel && !FINALIZED_STATUSES.includes(item.status);
  const showResumeRateLimited = !!onResumeRateLimited;
  const showViewBrief = !!(item.context && onViewContext);
  const showInfoEntries = entries.length > 0;
  const hasInfo = showResumeRateLimited || showViewBrief || showInfoEntries;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-xs" aria-label="More info">
          <MoreIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[220px]">
        {showResumeRateLimited && (
          <DropdownMenuItem disabled={resumeRateLimitedPending} onSelect={onResumeRateLimited}>
            {resumeRateLimitedPending ? 'Resuming…' : 'Resume task'}
          </DropdownMenuItem>
        )}
        {showResumeRateLimited && (showCancel || showViewBrief || showInfoEntries) && (
          <DropdownMenuSeparator />
        )}
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

/**
 * Opens the worktree's evidence deck in the default browser. The worker only
 * writes the deck once it has evidence to show, so a missing file surfaces as
 * the standard native-action toast rather than being hidden.
 */
export function EvidenceDeckButton({ worktree }: { worktree: string }): React.ReactElement {
  const { openLocalPath } = useNativeActions().files;

  return (
    <Button
      variant="ghost"
      size="xs"
      className="-ml-2 h-5 self-center justify-self-start text-caption text-text-2"
      onClick={() => openLocalPath(evidenceDeckPath(worktree))}
      data-testid="evidence-deck-open"
      title="Open the worker's evidence deck in your browser"
    >
      <Presentation className="text-text-3" />
      Open deck
    </Button>
  );
}
