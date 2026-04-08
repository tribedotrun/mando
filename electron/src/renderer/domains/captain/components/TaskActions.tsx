import React from 'react';
import type { TaskItem } from '#renderer/types';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';

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
