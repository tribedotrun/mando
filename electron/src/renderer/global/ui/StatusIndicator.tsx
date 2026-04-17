import React from 'react';
import { Ban, CircleAlert, CircleCheck, CircleDot, CircleHelp, CircleX } from 'lucide-react';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/global/ui/tooltip';
import { STATUS_TOOLTIP } from '#renderer/global/service/statusDisplay';
import {
  IconQueued,
  IconWorking,
  IconReviewing,
  IconRework,
  IconHandedOff,
} from '#renderer/global/ui/icons';

export { ACTION_LABELS, STATUS_TOOLTIP } from '#renderer/global/service/statusDisplay';

const S = 16;

const ICON_MAP: Record<string, () => React.ReactElement> = {
  new: IconQueued,
  queued: IconQueued,
  clarifying: IconWorking,
  'in-progress': IconWorking,
  'captain-reviewing': IconReviewing,
  'captain-merging': IconReviewing,
  'awaiting-review': () => <CircleDot size={S} color="var(--review)" />,
  escalated: () => <CircleAlert size={S} color="var(--destructive)" />,
  'needs-clarification': () => <CircleHelp size={S} color="var(--needs-human)" />,
  rework: IconRework,
  'handed-off': IconHandedOff,
  errored: () => <CircleX size={S} color="var(--destructive)" />,
  merged: () => <CircleCheck size={S} color="var(--text-3)" />,
  'completed-no-pr': () => <CircleCheck size={S} color="var(--text-3)" />,
  'plan-ready': () => <CircleDot size={S} color="var(--review)" />,
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
