import React from 'react';
import { Ban, CircleAlert, CircleCheck, CircleDot, CircleHelp, CircleX } from 'lucide-react';
import type { TaskItem } from '#renderer/types';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';

/* ── Status indicator ── */

/** Human-action states get a subtle inline label before the title */
export const ACTION_LABELS: Record<string, { color: string; label: string }> = {
  'awaiting-review': { color: 'var(--success)', label: 'Review' },
  escalated: { color: 'var(--destructive)', label: 'Escalated' },
  'needs-clarification': { color: 'var(--needs-human)', label: 'Needs input' },
};

/** Human-readable tooltip for each status */
export const STATUS_TOOLTIP: Record<string, string> = {
  new: 'Queued',
  queued: 'Queued',
  clarifying: 'Clarifying',
  'in-progress': 'Working',
  'captain-reviewing': 'Reviewing',
  'captain-merging': 'Merging',
  'awaiting-review': 'Awaiting review',
  escalated: 'Escalated',
  'needs-clarification': 'Needs input',
  rework: 'Rework',
  'handed-off': 'Handed off',
  errored: 'Errored',
  merged: 'Merged',
  'completed-no-pr': 'Done',
  canceled: 'Canceled',
};

const S = 16; // icon size

/** Dotted circle -- queued / new (not started) */
function IconQueued() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle
        cx="8"
        cy="8"
        r="6"
        stroke="var(--text-3)"
        strokeWidth="1.5"
        strokeDasharray="2.5 2.5"
      />
    </svg>
  );
}

/** Half-filled circle -- in progress / clarifying */
function IconWorking() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--primary)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--primary)" />
    </svg>
  );
}

/** Three-quarter circle -- captain reviewing (almost done) */
function IconReviewing() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--primary)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12A6 6 0 0 1 2 8h6V2z" fill="var(--primary)" />
    </svg>
  );
}

/** Half circle orange -- rework */
function IconRework() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--stale)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--stale)" />
    </svg>
  );
}

/** Open circle -- handed off (parked) */
function IconHandedOff() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--text-3)" strokeWidth="1.5" />
    </svg>
  );
}

const ICON_MAP: Record<string, () => React.ReactElement> = {
  new: IconQueued,
  queued: IconQueued,
  clarifying: IconWorking,
  'in-progress': IconWorking,
  'captain-reviewing': IconReviewing,
  'captain-merging': IconReviewing,
  'awaiting-review': () => <CircleDot size={S} color="var(--success)" />,
  escalated: () => <CircleAlert size={S} color="var(--destructive)" />,
  'needs-clarification': () => <CircleHelp size={S} color="var(--needs-human)" />,
  rework: IconRework,
  'handed-off': IconHandedOff,
  errored: () => <CircleX size={S} color="var(--destructive)" />,
  merged: () => <CircleCheck size={S} color="var(--success)" />,
  'completed-no-pr': () => <CircleCheck size={S} color="var(--success)" />,
  canceled: () => <Ban size={S} color="var(--text-4)" />,
};

export function StatusIcon({ status }: { status: string }): React.ReactElement {
  const Icon = ICON_MAP[status] ?? IconQueued;
  const tip = STATUS_TOOLTIP[status] ?? status;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex w-4 shrink-0 items-center justify-center">
          <Icon />
        </span>
      </TooltipTrigger>
      <TooltipContent side="right" className="text-xs">
        {tip}
      </TooltipContent>
    </Tooltip>
  );
}

export function ActionBtn({
  label,
  onClick,
  testId,
  disabled,
  pending,
}: {
  label: string;
  onClick: () => void;
  testId?: string;
  disabled?: boolean;
  pending?: boolean;
}): React.ReactElement {
  const isDisabled = disabled || pending;
  return (
    <Button
      data-testid={testId}
      variant="outline"
      size="xs"
      onClick={onClick}
      disabled={isDisabled}
    >
      {pending ? '...' : label}
    </Button>
  );
}

export function TaskOverflowMenu({
  item,
  open,
  onOpenChange,
  onRework,
  onHandoff,
  onCancel,
  onRetry,
  onAnswer,
  children,
}: {
  item: TaskItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRework: () => void;
  onHandoff: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onAnswer: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  const showRework = ['awaiting-review', 'handed-off', 'escalated', 'errored'].includes(
    item.status,
  );
  const showHandoff = ['awaiting-review', 'escalated'].includes(item.status);
  const showRetry = item.status === 'errored';
  const showAnswer = item.status === 'needs-clarification';

  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {showRetry && <DropdownMenuItem onSelect={onRetry}>Retry</DropdownMenuItem>}
        {showAnswer && <DropdownMenuItem onSelect={onAnswer}>Answer</DropdownMenuItem>}
        {showRework && <DropdownMenuItem onSelect={onRework}>Rework (new PR)</DropdownMenuItem>}
        {showHandoff && <DropdownMenuItem onSelect={onHandoff}>Hand off to human</DropdownMenuItem>}
        <DropdownMenuItem variant="destructive" onSelect={onCancel}>
          Cancel task
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
