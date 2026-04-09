import React, { useState, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Check, ChevronDown } from 'lucide-react';
import { Button } from '#renderer/components/ui/button';
import { useConfig } from '#renderer/hooks/queries';
import { useConfigSave } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import type { MandoConfig } from '#renderer/types';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';
import {
  ClaudeCodeContent,
  TelegramContent,
  ProjectContent,
  type ClaudeCheckResult,
} from '#renderer/domains/onboarding/components/SetupStepContent';

interface SetupChecklistProps {
  onDismiss: () => void;
  onMinimize?: () => void;
}

// -- Step completion checks (cached across popover open/close) --

let cachedClaudeCheck: ClaudeCheckResult | null = null;

function useStepStates() {
  const { data: config } = useConfig();
  const saveMut = useConfigSave();
  const qc = useQueryClient();
  const hasBotToken = !!config?.env?.TELEGRAM_MANDO_BOT_TOKEN;
  const [claudeResult, setClaudeResult] = useState<ClaudeCheckResult | null>(cachedClaudeCheck);

  const persistClaudeOk = useCallback(
    (result: ClaudeCheckResult) => {
      if (result.installed && result.works) {
        if (!config?.features?.claudeCodeVerified) {
          const current = qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? {};
          const updated: MandoConfig = {
            ...current,
            features: { ...(current.features || {}), claudeCodeVerified: true },
          };
          saveMut.mutate(updated);
        }
      }
    },
    [config?.features?.claudeCodeVerified, saveMut, qc],
  );

  useMountEffect(() => {
    if (cachedClaudeCheck !== null) return;
    window.mandoAPI
      ?.checkClaudeCode?.()
      .then((v) => {
        cachedClaudeCheck = v;
        setClaudeResult(v);
        persistClaudeOk(v);
      })
      .catch((err) => {
        log.error('checkClaudeCode failed:', err);
        const fail: ClaudeCheckResult = {
          installed: false,
          version: null,
          works: false,
          checkFailed: true,
          error: getErrorMessage(err, 'Unknown error'),
        };
        cachedClaudeCheck = fail;
        setClaudeResult(fail);
      });
  });

  const recheckClaude = useCallback(() => {
    cachedClaudeCheck = null;
    setClaudeResult(null);
    window.mandoAPI
      ?.checkClaudeCode?.()
      .then((v) => {
        cachedClaudeCheck = v;
        setClaudeResult(v);
        persistClaudeOk(v);
      })
      .catch((err) => {
        log.error('checkClaudeCode failed:', err);
        const fail: ClaudeCheckResult = {
          installed: false,
          version: null,
          works: false,
          checkFailed: true,
          error: getErrorMessage(err, 'Unknown error'),
        };
        cachedClaudeCheck = fail;
        setClaudeResult(fail);
      });
  }, [persistClaudeOk]);

  const claudeOk =
    (claudeResult?.installed === true && claudeResult.works === true) ||
    config?.features?.claudeCodeVerified === true;

  return {
    claudeResult,
    claudeOk,
    recheckClaude,
    hasProject: Object.keys(config?.captain?.projects ?? {}).length > 0,
    hasTelegram: !!(config?.channels?.telegram?.enabled && hasBotToken),
  };
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
    <div className="p-3">
      <h2 className="mb-2.5 text-[13px] font-semibold text-foreground">Get started with Mando</h2>

      {/* Progress bar */}
      <div className="mb-3 flex gap-px">
        {steps.map((_, i) => (
          <div
            key={i}
            className={`h-[3px] flex-1 rounded transition-colors duration-300 ${steps[i].completed ? 'bg-success' : 'bg-secondary'}`}
          />
        ))}
      </div>

      {/* Steps */}
      <div className="flex flex-col">
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
      <div className="mt-3 flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">
          {completedCount} of {steps.length} complete
        </span>
        {allComplete ? (
          <Button
            size="sm"
            onClick={onDismiss}
            className="bg-success font-semibold text-background hover:bg-success/90"
          >
            Done
          </Button>
        ) : (
          <Button variant="ghost" size="sm" onClick={onMinimize ?? onDismiss}>
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
