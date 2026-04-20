import React from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import {
  ClaudeCodeContent,
  TelegramContent,
  ProjectContent,
  type ClaudeCheckResult,
} from '#renderer/domains/onboarding/ui/SetupStepContent';

export type StepId = 'project' | 'claude-code' | 'telegram';

export interface StepDef {
  id: StepId;
  title: string;
  completed: boolean;
  expandable: boolean;
}

const STEP_GRID = 'grid grid-cols-[16px_1fr_14px] items-center gap-x-2';

export function StepRow({
  step,
  expanded,
  onToggle,
  recheckClaude,
  claudeResult,
}: {
  step: StepDef;
  expanded: boolean;
  onToggle: () => void;
  recheckClaude: () => void;
  claudeResult: ClaudeCheckResult | null;
}): React.ReactElement {
  const interactive = step.expandable && !step.completed;
  return (
    <div>
      {/* Step header */}
      <Button
        variant="ghost"
        className={`${STEP_GRID} h-auto w-full rounded-none py-2 ${interactive ? 'cursor-pointer' : 'cursor-default'}`}
        onClick={interactive ? onToggle : undefined}
        disabled={!interactive}
      >
        <StepIndicator completed={step.completed} />
        <span
          className={`text-left text-[12px] ${step.completed ? 'font-normal text-muted-foreground line-through' : 'font-medium text-foreground'}`}
        >
          {step.title}
        </span>
        <span>
          {step.expandable && !step.completed && (
            <ChevronDown
              size={12}
              className={`text-muted-foreground opacity-50 transition-transform duration-150 ${expanded ? 'rotate-180' : ''}`}
            />
          )}
        </span>
      </Button>

      {/* Expanded content */}
      {expanded && (
        <div className={`${STEP_GRID} items-start pb-2`}>
          <span />
          <div className="col-[2/-1]">
            {step.id === 'claude-code' && (
              <ClaudeCodeContent recheckClaude={recheckClaude} checkResult={claudeResult} />
            )}
            {step.id === 'telegram' && <TelegramContent />}
            {step.id === 'project' && <ProjectContent />}
          </div>
        </div>
      )}
    </div>
  );
}

export function StepIndicator({ completed }: { completed: boolean }): React.ReactElement {
  if (completed) {
    return (
      <div className="flex size-4 shrink-0 items-center justify-center rounded-full bg-success">
        <Check size={9} color="var(--background)" strokeWidth={3} />
      </div>
    );
  }
  return <div className="size-4 shrink-0 rounded-full bg-muted-foreground/20" />;
}
