import React from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  ClaudeCodeContent,
  TelegramContent,
  ProjectContent,
  type ClaudeCheckResult,
} from '#renderer/domains/onboarding/ui/SetupStepContent';
import { StepIndicator } from '#renderer/domains/onboarding/ui/SetupChecklistParts/StepIndicator';
import type { StepDef } from '#renderer/domains/onboarding/ui/SetupChecklistParts/types';

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
