import React, { useState, useCallback } from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import {
  ClaudeCodeContent,
  TelegramContent,
  ProjectContent,
  LinearContent,
  type ClaudeCheckResult,
} from '#renderer/components/SetupStepContent';

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
      .catch(() => {
        const fail = { installed: false, version: null, works: false };
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
      .catch(() => {
        const fail = { installed: false, version: null, works: false };
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
    hasLinear: !!(
      config.features?.linear &&
      config.captain?.linearTeam &&
      config.env?.LINEAR_API_KEY
    ),
  };
}

type StepId = 'project' | 'claude-code' | 'telegram' | 'linear';

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
    {
      id: 'linear',
      title: 'Connect Linear for task sync',
      completed: states.hasLinear,
      expandable: true,
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
      <h2
        className="text-[13px] font-semibold"
        style={{ color: 'var(--color-text-1)', marginBottom: 10 }}
      >
        Get started with Mando
      </h2>

      {/* Progress bar — fills left-to-right by count */}
      <div className="flex" style={{ gap: 1, marginBottom: 12 }}>
        {steps.map((_, i) => (
          <div
            key={i}
            style={{
              flex: 1,
              height: 3,
              borderRadius: 1.5,
              background: i < completedCount ? 'var(--color-success)' : 'var(--color-surface-3)',
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
        <span className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
          {completedCount} of {steps.length} complete
        </span>
        {allComplete ? (
          <button
            onClick={onDismiss}
            className="text-[12px] font-semibold"
            style={{
              padding: '5px 14px',
              borderRadius: 6,
              background: 'var(--color-success)',
              color: 'var(--color-bg)',
              border: 'none',
              cursor: 'pointer',
            }}
          >
            Done
          </button>
        ) : (
          <button
            onClick={onMinimize ?? onDismiss}
            className="text-[12px]"
            style={{
              padding: '5px 12px',
              color: 'var(--color-text-3)',
              background: 'none',
              border: 'none',
              cursor: 'pointer',
            }}
          >
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
          padding: '7px 0',
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
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="var(--color-text-3)"
              style={{
                transform: expanded ? 'rotate(180deg)' : 'none',
                transition: 'transform 0.15s ease',
                opacity: 0.5,
              }}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 9l-7 7-7-7"
              />
            </svg>
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
            {step.id === 'linear' && <LinearContent />}
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
        <svg
          width="9"
          height="9"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--color-bg)"
          strokeWidth={3}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M20 6L9 17l-5-5" />
        </svg>
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
