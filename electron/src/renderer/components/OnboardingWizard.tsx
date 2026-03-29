import React, { useState, useCallback } from 'react';
import type { MandoConfig } from '#renderer/stores/settingsStore';
import { PreviewPane, type Feature } from '#renderer/components/OnboardingPreview';
import {
  INPUT_CLS,
  INPUT_STYLE,
  CenteredCard,
  StatusCard,
  CheckRow,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/components/OnboardingPrimitives';
import { getErrorMessage } from '#renderer/utils';

type Step = 'welcome' | 'claude-check' | 'telegram' | 'finishing';

type CCResult = { installed: boolean; version: string | null; works: boolean } | null;
type TGResult = { botUsername?: string; error?: string } | null;

const FEATURES: { id: Feature; label: string; desc: string }[] = [
  {
    id: 'captain',
    label: 'CAPTAIN',
    desc: 'Your AI dev team. Add tasks, agents code and merge PRs autonomously.',
  },
  {
    id: 'scout',
    label: 'SCOUT',
    desc: 'Curated learning. AI processes articles and repos you care about.',
  },
  {
    id: 'sessions',
    label: 'SESSIONS',
    desc: 'Full audit trail. Every agent action, cost, and transcript.',
  },
];

export function OnboardingWizard(): React.ReactElement {
  const [step, setStep] = useState<Step>('welcome');
  const [selectedFeature, setSelectedFeature] = useState<Feature>('captain');
  const [error, setError] = useState<string | null>(null);
  const [tgToken, setTgToken] = useState('');

  const finishSetup = useCallback(
    async (includeTg: boolean) => {
      setError(null);
      setStep('finishing');
      try {
        const config: MandoConfig = { features: { claudeCodeVerified: true } };
        if (includeTg && tgToken.trim()) {
          config.channels = { telegram: { enabled: true } };
          config.env = { TELEGRAM_MANDO_BOT_TOKEN: tgToken.trim() };
        }
        await window.mandoAPI.setupComplete(JSON.stringify(config, null, 2));
        window.location.reload();
      } catch (err) {
        setError(getErrorMessage(err, 'Failed to save configuration'));
        setStep('telegram');
      }
    },
    [tgToken],
  );

  if (step === 'welcome') {
    return (
      <WelcomeScreen
        selected={selectedFeature}
        onSelect={setSelectedFeature}
        onStart={() => setStep('claude-check')}
      />
    );
  }

  if (step === 'claude-check') {
    return (
      <ClaudeCheckScreen onBack={() => setStep('welcome')} onPass={() => setStep('telegram')} />
    );
  }

  return (
    <TelegramScreen
      token={tgToken}
      onTokenChange={setTgToken}
      onBack={() => setStep('claude-check')}
      onSkip={() => finishSetup(false)}
      onFinish={() => finishSetup(true)}
      error={error}
      finishing={step === 'finishing'}
    />
  );
}

// ---- Welcome screen ----

function WelcomeScreen({
  selected,
  onSelect,
  onStart,
}: {
  selected: Feature;
  onSelect: (f: Feature) => void;
  onStart: () => void;
}): React.ReactElement {
  return (
    <div
      data-testid="onboarding-wizard"
      className="flex h-full"
      style={{ background: 'var(--color-bg)' }}
    >
      <div className="flex flex-col" style={{ width: 420, padding: '48px 40px', flexShrink: 0 }}>
        <div style={{ marginBottom: 40 }}>
          <h1 className="text-display" style={{ color: 'var(--color-text-1)' }}>
            Mando
          </h1>
          <p className="text-body" style={{ color: 'var(--color-text-2)', marginTop: 4 }}>
            AI agents that ship your code
          </p>
        </div>
        <div className="flex flex-col" style={{ gap: 12 }}>
          {FEATURES.map((f) => {
            const on = selected === f.id;
            return (
              <button
                key={f.id}
                onClick={() => onSelect(f.id)}
                className="flex flex-col transition-colors"
                style={{
                  padding: '16px 20px',
                  borderRadius: 'var(--radius-panel)',
                  border: `1px solid ${on ? 'var(--color-border)' : 'var(--color-border-subtle)'}`,
                  background: 'var(--color-surface-1)',
                  cursor: 'pointer',
                  textAlign: 'left',
                }}
              >
                <div className="flex items-center" style={{ gap: 8, marginBottom: 8 }}>
                  <span
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: 4,
                      flexShrink: 0,
                      background: on ? 'var(--color-accent)' : 'transparent',
                      border: on ? 'none' : '1.5px solid var(--color-border)',
                    }}
                  />
                  <span className="text-label" style={{ color: 'var(--color-text-2)' }}>
                    {f.label}
                  </span>
                </div>
                <p className="text-body" style={{ color: 'var(--color-text-3)' }}>
                  {f.desc}
                </p>
              </button>
            );
          })}
        </div>
        <div className="flex-1" />
        <div className="flex justify-center" style={{ paddingTop: 32 }}>
          <button
            data-testid="onboarding-next"
            onClick={onStart}
            className="text-[13px] font-semibold"
            style={{
              padding: '10px 32px',
              borderRadius: 'var(--radius-button)',
              background: 'var(--color-accent)',
              color: 'var(--color-bg)',
              border: 'none',
              cursor: 'pointer',
            }}
          >
            Get Started
          </button>
        </div>
      </div>
      <div
        className="flex flex-1 items-start justify-center"
        style={{
          background: 'var(--color-surface-1)',
          borderLeft: '1px solid var(--color-border-subtle)',
          padding: 24,
          overflow: 'auto',
        }}
      >
        <PreviewPane feature={selected} />
      </div>
    </div>
  );
}

// ---- Claude Code check ----

function ClaudeCheckScreen({
  onBack,
  onPass,
}: {
  onBack: () => void;
  onPass: () => void;
}): React.ReactElement {
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<CCResult>(null);

  const runCheck = useCallback(async () => {
    setChecking(true);
    setResult(null);
    try {
      setResult(await window.mandoAPI.checkClaudeCode());
    } catch {
      setResult({ installed: false, version: null, works: false });
    } finally {
      setChecking(false);
    }
  }, []);

  const mountRef = React.useRef(false);
  if (!mountRef.current) {
    mountRef.current = true;
    runCheck();
  }

  const passed = result?.installed === true && result.works === true;

  return (
    <CenteredCard data-testid="onboarding-wizard">
      <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 8 }}>
        Claude Code
      </h2>
      <p className="text-body" style={{ color: 'var(--color-text-2)', marginBottom: 24 }}>
        Mando uses Claude Code to run AI agents. Let&apos;s make sure it&apos;s working.
      </p>

      <StatusCard>
        {checking && (
          <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
            Checking…
          </span>
        )}
        {result && !checking && (
          <>
            <CheckRow ok={result.installed} label="Installed" />
            {result.version && (
              <span
                className="text-caption"
                style={{ color: 'var(--color-text-3)', paddingLeft: 24 }}
              >
                {result.version}
              </span>
            )}
            {result.installed && (
              <CheckRow
                ok={result.works}
                label={
                  result.works ? 'Responding' : 'Not responding — check your Anthropic API key'
                }
              />
            )}
          </>
        )}
      </StatusCard>

      {result && !result.installed && !checking && (
        <p className="text-body" style={{ color: 'var(--color-text-2)', marginBottom: 16 }}>
          Install Claude Code from{' '}
          <a
            href="https://docs.anthropic.com/en/docs/claude-code/overview"
            target="_blank"
            rel="noopener noreferrer"
            style={{ color: 'var(--color-accent)' }}
          >
            the docs
          </a>
          , then re-check.
        </p>
      )}

      <div className="flex items-center" style={{ gap: 8 }}>
        <GhostButton onClick={onBack}>Back</GhostButton>
        {!passed && (
          <OutlineButton onClick={runCheck} disabled={checking}>
            {checking ? 'Checking…' : 'Re-check'}
          </OutlineButton>
        )}
        <PrimaryButton onClick={onPass} disabled={!passed} variant="success">
          Continue
        </PrimaryButton>
      </div>
    </CenteredCard>
  );
}

// ---- Telegram setup (skippable) ----

function TelegramScreen({
  token,
  onTokenChange,
  onBack,
  onSkip,
  onFinish,
  error,
  finishing,
}: {
  token: string;
  onTokenChange: (v: string) => void;
  onBack: () => void;
  onSkip: () => void;
  onFinish: () => void;
  error: string | null;
  finishing: boolean;
}): React.ReactElement {
  const [validating, setValidating] = useState(false);
  const [tgResult, setTgResult] = useState<TGResult>(null);

  const validate = useCallback(async () => {
    setValidating(true);
    setTgResult(null);
    try {
      const res = await window.mandoAPI.validateTelegramToken(token.trim());
      setTgResult(
        res.valid ? { botUsername: res.botUsername } : { error: res.error ?? 'Invalid token' },
      );
    } catch {
      setTgResult({ error: 'Validation failed' });
    } finally {
      setValidating(false);
    }
  }, [token]);

  const tokenValid = !!tgResult?.botUsername;
  const canFinish = tokenValid && !finishing;

  return (
    <CenteredCard data-testid="onboarding-wizard">
      <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 8 }}>
        Telegram
      </h2>
      <p className="text-body" style={{ color: 'var(--color-text-2)', marginBottom: 24 }}>
        Control Mando from your phone. Create a bot via{' '}
        <a
          href="https://t.me/BotFather"
          target="_blank"
          rel="noopener noreferrer"
          style={{ color: 'var(--color-accent)' }}
        >
          @BotFather
        </a>{' '}
        and paste the token below.
      </p>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 20 }}>
        <div className="flex" style={{ gap: 8 }}>
          <input
            className={INPUT_CLS}
            style={INPUT_STYLE}
            value={token}
            onChange={(e) => {
              onTokenChange(e.target.value);
              setTgResult(null);
            }}
            placeholder="Bot token"
          />
          <OutlineButton onClick={validate} disabled={!token.trim() || validating}>
            {validating ? 'Checking…' : 'Verify'}
          </OutlineButton>
        </div>
        {tgResult?.botUsername && <CheckRow ok label={`@${tgResult.botUsername}`} />}
        {tgResult?.error && <CheckRow ok={false} label={tgResult.error} />}
      </div>

      {error && (
        <div
          className="text-body"
          style={{
            marginBottom: 16,
            padding: '8px 16px',
            borderRadius: 'var(--radius-button)',
            background: 'var(--color-error-bg)',
            color: 'var(--color-error)',
          }}
        >
          {error}
        </div>
      )}

      <div className="flex items-center" style={{ gap: 8 }}>
        <GhostButton onClick={onBack}>Back</GhostButton>
        <GhostButton onClick={onSkip}>Skip</GhostButton>
        <PrimaryButton onClick={onFinish} disabled={!canFinish} variant="success">
          {finishing ? 'Setting up…' : 'Finish Setup'}
        </PrimaryButton>
      </div>
    </CenteredCard>
  );
}
