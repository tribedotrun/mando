import React, { useState, useCallback } from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { useSettingsStore } from '#renderer/domains/settings';
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
  const config = useSettingsStore((s) => s.config);
  const hasBotToken = useSettingsStore((s) => !!s.config.env?.TELEGRAM_MANDO_BOT_TOKEN);
  const [claudeResult, setClaudeResult] = useState<ClaudeCheckResult | null>(cachedClaudeCheck);

  const persistClaudeOk = useCallback((result: ClaudeCheckResult) => {
    if (result.installed && result.works) {
      const store = useSettingsStore.getState();
      if (!store.config.features?.claudeCodeVerified) {
        store.updateSection('features', { claudeCodeVerified: true });
        store.save();
      }
    }
  }, []);

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
    config.features?.claudeCodeVerified === true;

  return {
    claudeResult,
    claudeOk,
    recheckClaude,
    hasProject: Object.keys(config.captain?.projects ?? {}).length > 0,
    hasTelegram: !!(config.channels?.telegram?.enabled && hasBotToken),
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
  const loaded = useSettingsStore((s) => s.loaded);
  const load = useSettingsStore((s) => s.load);
  const states = useStepStates();
  const [userExpandedStep, setUserExpandedStep] = useState<StepId | null>(null);

  useMountEffect(() => {
    if (!loaded) load();
  });

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

  if (!loaded) return <div />;

  return (
    <div style={{ padding: 12 }}>
      <h2 className="text-[13px] font-semibold text-text-1" style={{ marginBottom: 10 }}>
        Get started with Mando
      </h2>

      {/* Progress bar — each segment maps to its step */}
      <div className="flex" style={{ gap: 1, marginBottom: 12 }}>
        {steps.map((_, i) => (
          <div
            key={i}
            style={{
              flex: 1,
              height: 3,
              borderRadius: 4,
              background: steps[i].completed ? 'var(--color-success)' : 'var(--color-surface-3)',
              transition: 'background 0.3s ease',
            }}
          />
        ))}
      </div>

      {/* Steps */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 0 }}>
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
      <div className="flex items-center justify-between" style={{ marginTop: 12 }}>
        <span className="text-[11px] text-text-3">
          {completedCount} of {steps.length} complete
        </span>
        {allComplete ? (
          <button
            onClick={onDismiss}
            className="btn"
            style={{
              background: 'var(--color-success)',
              color: 'var(--color-bg)',
              fontWeight: 600,
              border: 'none',
            }}
          >
            Done
          </button>
        ) : (
          <button onClick={onMinimize ?? onDismiss} className="btn btn-ghost">
            Later
          </button>
        )}
      </div>
    </div>
  );
}

// -- Step row — grid layout for guaranteed column alignment --

const STEP_GRID: React.CSSProperties = {
  display: 'grid',
  gridTemplateColumns: '16px 1fr 14px',
  columnGap: 8,
  alignItems: 'center',
};

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
      {/* Step header — 3-column grid: [indicator] [title] [chevron] */}
      <button
        style={{
          ...STEP_GRID,
          width: '100%',
          padding: '8px 0',
          background: 'transparent',
          border: 'none',
          cursor: step.expandable ? 'pointer' : 'default',
        }}
        onClick={onToggle}
      >
        <StepIndicator completed={step.completed} />
        <span
          className="text-[12px]"
          style={{
            color: step.completed ? 'var(--color-text-3)' : 'var(--color-text-1)',
            textDecoration: step.completed ? 'line-through' : 'none',
            fontWeight: step.completed ? 400 : 500,
            textAlign: 'left',
          }}
        >
          {step.title}
        </span>
        <span>
          {step.expandable && !step.completed && (
            <ChevronDown
              size={12}
              color="var(--color-text-3)"
              style={{
                transform: expanded ? 'rotate(180deg)' : 'none',
                transition: 'transform 0.15s ease',
                opacity: 0.5,
              }}
            />
          )}
        </span>
      </button>

      {/* Expanded content — spans columns 2-3 to align with title */}
      {expanded && (
        <div
          style={{
            ...STEP_GRID,
            alignItems: 'start',
            paddingBottom: 8,
          }}
        >
          <span />
          <div style={{ gridColumn: '2 / -1' }}>
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
      <div
        className="flex shrink-0 items-center justify-center"
        style={{ width: 16, height: 16, borderRadius: 8, background: 'var(--color-success)' }}
      >
        <Check size={9} color="var(--color-bg)" strokeWidth={3} />
      </div>
    );
  }
  return (
    <div
      style={{
        width: 16,
        height: 16,
        borderRadius: 8,
        border: '1.5px solid var(--color-border)',
        flexShrink: 0,
      }}
    />
  );
}
