import React, { useState, useCallback } from 'react';
import { Button } from '#renderer/global/ui/button';
import { useConfig } from '#renderer/domains/onboarding/runtime/hooks';
import { useStepStates } from '#renderer/domains/onboarding/runtime/useStepStates';
import {
  StepRow,
  type StepId,
  type StepDef,
} from '#renderer/domains/onboarding/ui/SetupChecklistParts';

interface SetupChecklistProps {
  onDismiss: () => void;
  onMinimize?: () => void;
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
