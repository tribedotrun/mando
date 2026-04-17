import React, { useState, useCallback } from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import { useConfig } from '#renderer/domains/onboarding/runtime/hooks';
import { useStepStates } from '#renderer/domains/onboarding/runtime/useStepStates';
import {
  ClaudeCodeContent,
  TelegramContent,
  ProjectContent,
  type ClaudeCheckResult,
} from '#renderer/domains/onboarding/ui/SetupStepContent';

interface SetupChecklistProps {
  onDismiss: () => void;
  onMinimize?: () => void;
}

type StepId = 'project' | 'claude-code' | 'telegram';

interface StepDef {
  id: StepId;
  title: string;
  completed: boolean;
  expandable: boolean;
}

export function SetupChecklist({ onDismiss, onMinimize }: SetupChecklistProps): React.ReactElement {
  const { data: config, isLoading } = useConfig();
  const states = useStepStates();
  const [userExpandedStep, setUserExpandedStep] = useState<StepId | null>(null);

  const steps: StepDef[] = [
    {
      id: 'claude-code',
      title: 'Install Claude Code',
      completed: states.claudeOk,
      expandable: !states.claudeOk,
    },
    {
      id: 'telegram',
      title: 'Connect Telegram for remote control',
      completed: states.hasTelegram,
      expandable: true,
    },
    {
      id: 'project',
      title: 'Add a project',
      completed: states.hasProject,
      expandable: !states.hasProject,
    },
  ];

  const completedCount = steps.filter((s) => s.completed).length;
  const allComplete = completedCount === steps.length;
  const autoExpand = steps.find((s) => !s.completed && s.expandable)?.id ?? null;
  const expandedStep = userExpandedStep ?? autoExpand;

  const toggleStep = useCallback((id: StepId) => {
    setUserExpandedStep((prev) => (prev === id ? null : id));
  }, []);

  if (isLoading || !config) return <div />;

  return (
    <div className="p-4">
      <h2 className="mb-3 text-[13px] font-semibold text-foreground">Get started with Mando</h2>

      {/* Progress bar */}
      <div className="mb-4 flex gap-1">
        {steps.map((_, i) => (
          <div
            key={i}
            className={`h-[3px] flex-1 rounded-full transition-colors duration-300 ${steps[i].completed ? 'bg-success' : 'bg-muted-foreground/20'}`}
          />
        ))}
      </div>

      {/* Steps */}
      <div className="flex flex-col gap-0.5">
        {steps.map((step) => (
          <StepRow
            key={step.id}
            step={step}
            expanded={expandedStep === step.id}
            onToggle={() => step.expandable && toggleStep(step.id)}
            recheckClaude={states.recheckClaude}
            claudeResult={states.claudeResult}
          />
        ))}
      </div>

      {/* Footer */}
      <div className="mt-4 flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">
          {completedCount} of {steps.length} complete
        </span>
        {allComplete ? (
          <Button
            size="xs"
            onClick={onDismiss}
            className="bg-success font-semibold text-background hover:bg-success/90"
          >
            Done
          </Button>
        ) : (
          <Button variant="ghost" size="xs" onClick={onMinimize ?? onDismiss}>
            Later
          </Button>
        )}
      </div>
    </div>
  );
}

// -- Step row --

const STEP_GRID = 'grid grid-cols-[16px_1fr_14px] items-center gap-x-2';

function StepRow({
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
  return (
    <div>
      {/* Step header */}
      <Button
        variant="ghost"
        className={`${STEP_GRID} h-auto w-full rounded-none py-2 ${step.expandable ? 'cursor-pointer' : 'cursor-default'}`}
        onClick={onToggle}
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

function StepIndicator({ completed }: { completed: boolean }): React.ReactElement {
  if (completed) {
    return (
      <div className="flex size-4 shrink-0 items-center justify-center rounded-full bg-success">
        <Check size={9} color="var(--background)" strokeWidth={3} />
      </div>
    );
  }
  return <div className="size-4 shrink-0 rounded-full bg-muted-foreground/20" />;
}
