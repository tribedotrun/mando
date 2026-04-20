import React from 'react';
import { ArrowUp } from 'lucide-react';
import { shortRepo } from '#renderer/global/service/utils';
import { Combobox } from '#renderer/global/ui/combobox';
import { Switch } from '#renderer/global/ui/switch';
import { Button } from '#renderer/global/ui/button';
import { SpinnerIcon } from '#renderer/global/ui/SpinnerIcon';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/global/ui/tooltip';
import { TaskAttachmentButton } from '#renderer/domains/captain/ui/TaskComposerControlsParts';

export { TaskAttachmentButton };

interface TaskProjectSelectProps {
  projects: string[];
  value: string;
  onValueChange: (value: string) => void;
  testId: string;
}

export function TaskProjectSelect({
  projects,
  value,
  onValueChange,
  testId,
}: TaskProjectSelectProps): React.ReactElement | null {
  if (projects.length === 0) return null;

  return (
    <Combobox
      data-testid={testId}
      value={value}
      onValueChange={onValueChange}
      options={projects.map((item) => ({
        value: item,
        label: shortRepo(item),
      }))}
      placeholder="Project..."
      searchPlaceholder="Search projects..."
      emptyText="No projects found."
    />
  );
}

interface TaskAutoMergeToggleProps {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  className?: string;
}

export function TaskAutoMergeToggle({
  checked,
  onCheckedChange,
  className,
}: TaskAutoMergeToggleProps): React.ReactElement {
  return (
    <label className={className ?? 'flex items-center gap-1.5 text-[12px] text-muted-foreground'}>
      <Switch checked={checked} onCheckedChange={onCheckedChange} className="scale-75" />
      Skip auto-merge
    </label>
  );
}

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
