import React from 'react';
import type { TaskItem } from '#renderer/global/types';
import {
  canRework,
  canHandoff,
  canRetry,
  canAnswer,
  canStop,
} from '#renderer/global/service/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';

export function TaskOverflowMenu({
  item,
  open,
  onOpenChange,
  onRework,
  onHandoff,
  onStop,
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
  onStop: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onAnswer: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  const showRework = canRework(item);
  const showHandoff = canHandoff(item);
  const showStop = canStop(item);
  const showRetry = canRetry(item);
  const showAnswer = canAnswer(item);

  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {showRetry && <DropdownMenuItem onSelect={onRetry}>Retry</DropdownMenuItem>}
        {showAnswer && <DropdownMenuItem onSelect={onAnswer}>Answer</DropdownMenuItem>}
        {showRework && <DropdownMenuItem onSelect={onRework}>Rework (new PR)</DropdownMenuItem>}
        {showHandoff && <DropdownMenuItem onSelect={onHandoff}>Hand off to human</DropdownMenuItem>}
        {showStop && (
          <DropdownMenuItem variant="destructive" onSelect={onStop}>
            Stop worker
          </DropdownMenuItem>
        )}
        <DropdownMenuItem variant="destructive" onSelect={onCancel}>
          Cancel task
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
