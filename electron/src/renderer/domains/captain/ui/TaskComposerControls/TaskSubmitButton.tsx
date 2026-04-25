import React from 'react';
import { ArrowUp } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { SpinnerIcon } from '#renderer/domains/captain/ui/SpinnerIcon';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/global/ui/primitives/tooltip';

interface TaskSubmitButtonProps {
  disabled: boolean;
  pending: boolean;
  onSubmit: () => void;
  testId?: string;
  tooltip?: string;
  className?: string;
  ariaLabel?: string;
  variant?: 'default' | 'secondary';
}

export function TaskSubmitButton({
  disabled,
  pending,
  onSubmit,
  testId,
  tooltip = 'Create ⌘↵',
  className,
  ariaLabel = 'Create task',
  variant = 'default',
}: TaskSubmitButtonProps): React.ReactElement {
  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            data-testid={testId}
            onClick={onSubmit}
            disabled={disabled}
            variant={variant}
            size="icon-xs"
            aria-label={ariaLabel}
            className={className ?? 'shrink-0 rounded-full transition-colors'}
          >
            {pending ? <SpinnerIcon /> : <ArrowUp size={14} strokeWidth={2} />}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top" className="text-xs">
          {tooltip}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
