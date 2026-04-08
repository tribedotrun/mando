import React from 'react';
import { Ban, CircleAlert, CircleCheck, CircleDot, CircleHelp, CircleX } from 'lucide-react';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';
import {
  IconQueued,
  IconWorking,
  IconReviewing,
  IconRework,
  IconHandedOff,
} from '#renderer/global/components/icons';

const S = 16;

/** Human-action states get a subtle inline label before the title */
export const ACTION_LABELS: Record<string, { color: string; label: string }> = {
  'awaiting-review': { color: 'var(--muted-foreground)', label: 'Review' },
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

const ICON_MAP: Record<string, () => React.ReactElement> = {
  new: IconQueued,
  queued: IconQueued,
  clarifying: IconWorking,
  'in-progress': IconWorking,
  'captain-reviewing': IconReviewing,
  'captain-merging': IconReviewing,
  'awaiting-review': () => <CircleDot size={S} color="var(--muted-foreground)" />,
  escalated: () => <CircleAlert size={S} color="var(--destructive)" />,
  'needs-clarification': () => <CircleHelp size={S} color="var(--needs-human)" />,
  rework: IconRework,
  'handed-off': IconHandedOff,
  errored: () => <CircleX size={S} color="var(--destructive)" />,
  merged: () => <CircleCheck size={S} color="var(--text-3)" />,
  'completed-no-pr': () => <CircleCheck size={S} color="var(--text-3)" />,
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
