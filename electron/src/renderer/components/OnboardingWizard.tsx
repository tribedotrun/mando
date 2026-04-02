import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import type { MandoConfig } from '#renderer/stores/settingsStore';
import heroImg from '#renderer/assets/hero.png';
import {
  SetupLayout,
  CheckRow,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/components/OnboardingPrimitives';
import { TelegramScreen, LinearScreen } from '#renderer/components/OnboardingSteps';
import { getErrorMessage } from '#renderer/utils';

type Step = 'welcome' | 'claude-check' | 'telegram' | 'linear' | 'finishing' | 'done';

type CCResult = { installed: boolean; version: string | null; works: boolean } | null;

const BULLETS = [
  'Your backlog runs itself — tasks get picked up, reviewed, and delivered as pull requests with visual evidence.',
  'Run tasks across multiple projects at once. Each gets its own isolated workspace.',
  'Stay current — relevant articles, repos, and podcasts become actionable tasks tailored to your stack.',
];

export function OnboardingWizard(): React.ReactElement {
  const [step, setStep] = useState<Step>('welcome');
  const [error, setError] = useState<string | null>(null);
  const [tgToken, setTgToken] = useState('');
  const [linearKey, setLinearKey] = useState('');
  const [linearTeam, setLinearTeam] = useState('');
  const [progressMsg, setProgressMsg] = useState<string | null>(null);

  useMountEffect(() => {
    window.mandoAPI.onSetupProgress(setProgressMsg);
  });

  /** Persist partial config to disk so progress survives a crash or quit. */
  const saveProgress = useCallback(
    (extras?: { linearKey?: string; linearTeam?: string }) => {
      const config: MandoConfig = { features: { claudeCodeVerified: true } };
      const env: Record<string, string> = {};
      if (tgToken.trim()) {
        config.channels = { telegram: { enabled: true } };
        env.TELEGRAM_MANDO_BOT_TOKEN = tgToken.trim();
      }
      if (extras?.linearKey?.trim() && extras.linearTeam) {
        config.features!.linear = true;
        config.captain = { linearTeam: extras.linearTeam };
        env.LINEAR_API_KEY = extras.linearKey.trim();
      }
      if (Object.keys(env).length > 0) config.env = env;
      window.mandoAPI
        .saveConfigLocal(JSON.stringify(config, null, 2))
        .catch((e) => console.error('Failed to save onboarding progress:', e));
    },
    [tgToken],
  );

  const finishSetup = useCallback(async () => {
    setError(null);
    setStep('finishing');
    try {
      const config: MandoConfig = {
        features: { claudeCodeVerified: true },
        captain: { autoSchedule: true },
      };
      const env: Record<string, string> = {};
      if (tgToken.trim()) {
        config.channels = { telegram: { enabled: true } };
        env.TELEGRAM_MANDO_BOT_TOKEN = tgToken.trim();
      }
      if (linearKey.trim() && linearTeam) {
        config.features!.linear = true;
        config.captain!.linearTeam = linearTeam;
        env.LINEAR_API_KEY = linearKey.trim();
      }
      if (Object.keys(env).length > 0) config.env = env;
      await window.mandoAPI.setupComplete(JSON.stringify(config, null, 2));
      setStep('done');
    } catch (err) {
      setError(getErrorMessage(err, 'Failed to save configuration'));
      setStep('linear');
    }
  }, [tgToken, linearKey, linearTeam]);

  if (step === 'welcome') {
    return <WelcomeScreen onStart={() => setStep('claude-check')} />;
  }

  if (step === 'claude-check') {
    return (
      <ClaudeCheckScreen onBack={() => setStep('welcome')} onPass={() => setStep('telegram')} />
    );
  }

  if (step === 'telegram') {
    return (
      <TelegramScreen
        token={tgToken}
        onTokenChange={setTgToken}
        onBack={() => setStep('claude-check')}
        onNext={() => {
          saveProgress();
          setStep('linear');
        }}
        onSkip={() => {
          setTgToken('');
          setStep('linear');
        }}
      />
    );
  }

  if (step === 'done') {
    return (
      <DoneScreen hasTelegram={!!tgToken.trim()} hasLinear={!!linearKey.trim() && !!linearTeam} />
    );
  }

  return (
    <LinearScreen
      apiKey={linearKey}
      onApiKeyChange={setLinearKey}
      selectedTeam={linearTeam}
      onTeamChange={setLinearTeam}
      onBack={() => setStep('telegram')}
      onFinish={finishSetup}
      error={error}
      finishing={step === 'finishing'}
      progressMsg={progressMsg}
    />
  );
}

// ---- Welcome screen ----

function WelcomeScreen({ onStart }: { onStart: () => void }): React.ReactElement {
  return (
    <div
      data-testid="onboarding-wizard"
      className="relative flex h-full"
      style={{ background: 'var(--color-bg)' }}
    >
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <div
        className="flex flex-col justify-center"
        style={{ width: 460, padding: 48, flexShrink: 0 }}
      >
        <div style={{ marginBottom: 24 }}>
          <h1 className="text-display" style={{ color: 'var(--color-text-1)' }}>
            Mando
          </h1>
          <p
            className="text-subheading"
            style={{ color: 'var(--color-text-2)', marginTop: 8, letterSpacing: '0.01em' }}
          >
            Manage tasks, not agents.
          </p>
        </div>
        <ul className="flex flex-col" style={{ gap: 24, listStyle: 'none', padding: 0, margin: 0 }}>
          {BULLETS.map((text, i) => (
            <li key={i} className="flex" style={{ gap: 12 }}>
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 3,
                  background: 'var(--color-accent)',
                  flexShrink: 0,
                  marginTop: 6,
                }}
              />
              <span className="text-body" style={{ color: 'var(--color-text-1)', lineHeight: 1.5 }}>
                {text}
              </span>
            </li>
          ))}
        </ul>
        <div style={{ paddingTop: 40 }}>
          <button
            data-testid="onboarding-next"
            onClick={onStart}
            className="text-[13px] font-semibold transition-colors hover:brightness-110 active:brightness-90"
            style={{
              padding: '8px 20px',
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
        className="flex flex-1 items-center justify-center"
        style={{ overflow: 'hidden', padding: '24px 24px 24px 0' }}
      >
        <img
          src={heroImg}
          alt="Mando captain view"
          style={{
            width: '100%',
            height: 'auto',
            objectFit: 'contain',
            borderRadius: 'var(--radius-panel)',
            border: '1px solid var(--color-border-subtle)',
            boxShadow: '0 8px 32px rgba(0, 0, 0, 0.35)',
          }}
        />
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
    <SetupLayout
      data-testid="onboarding-wizard"
      step={1}
      total={3}
      title="Claude Code"
      subtitle="Required to run Mando."
    >
      <div
        className="flex flex-col"
        style={{
          gap: 12,
          padding: '28px 28px',
          borderRadius: 'var(--radius-panel)',
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
          boxShadow: '0 1px 4px rgba(0, 0, 0, 0.2)',
          marginBottom: 40,
        }}
      >
        {checking && (
          <div className="flex items-center" style={{ gap: 10 }}>
            <span
              className="animate-spin"
              style={{
                width: 14,
                height: 14,
                borderRadius: 7,
                border: '2px solid var(--color-accent)',
                borderTopColor: 'transparent',
                flexShrink: 0,
              }}
            />
            <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
              Checking…
            </span>
          </div>
        )}
        {result && !checking && (
          <>
            <StatusCheck ok={result.installed} label="Installed" detail={result.version} />
            {result.installed && (
              <StatusCheck
                ok={result.works}
                label={result.works ? 'Responding' : 'Not responding, check your API key'}
              />
            )}
          </>
        )}
      </div>

      {result && !result.installed && !checking && (
        <p className="text-body" style={{ color: 'var(--color-text-2)', marginBottom: 24 }}>
          Install Claude Code from{' '}
          <a
            href="https://code.claude.com/docs/en/overview"
            target="_blank"
            rel="noopener noreferrer"
            style={{ color: 'var(--color-accent)' }}
          >
            the docs
          </a>
          , then re-check.
        </p>
      )}

      <div className="flex items-center" style={{ justifyContent: 'space-between' }}>
        <div className="flex items-center" style={{ gap: 12 }}>
          <GhostButton onClick={onBack}>Back</GhostButton>
          {!passed && (
            <OutlineButton onClick={runCheck} disabled={checking}>
              {checking ? 'Checking…' : 'Re-check'}
            </OutlineButton>
          )}
        </div>
        <PrimaryButton onClick={onPass} disabled={!passed}>
          Continue
        </PrimaryButton>
      </div>
    </SetupLayout>
  );
}

// ---- Done screen ----

function DoneScreen({
  hasTelegram,
  hasLinear,
}: {
  hasTelegram: boolean;
  hasLinear: boolean;
}): React.ReactElement {
  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      title="You're all set"
      subtitle="Mando is ready. Add a project and create your first task."
    >
      <div
        className="flex flex-col"
        style={{
          gap: 8,
          marginBottom: 40,
          padding: '28px 28px',
          borderRadius: 'var(--radius-panel)',
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
          boxShadow: '0 1px 4px rgba(0, 0, 0, 0.2)',
        }}
      >
        <CheckRow ok label="Claude Code" />
        <CheckRow ok={hasTelegram} label={hasTelegram ? 'Telegram' : 'Telegram — skipped'} />
        <CheckRow ok={hasLinear} label={hasLinear ? 'Linear' : 'Linear — skipped'} />
      </div>

      <PrimaryButton onClick={() => window.location.reload()}>Open Mando</PrimaryButton>
    </SetupLayout>
  );
}

function StatusCheck({
  ok,
  label,
  detail,
}: {
  ok: boolean;
  label: string;
  detail?: string | null;
}): React.ReactElement {
  return (
    <div className="flex items-center" style={{ gap: 10 }}>
      <span
        style={{
          width: 16,
          height: 16,
          borderRadius: 8,
          background: ok ? 'rgba(74, 180, 100, 0.85)' : 'var(--color-danger)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0,
          fontSize: 10,
          color: '#fff',
          fontWeight: 700,
        }}
      >
        {ok ? '✓' : '✗'}
      </span>
      <span
        className="text-body"
        style={{ color: ok ? 'var(--color-text-1)' : 'var(--color-danger)' }}
      >
        {label}
      </span>
      {detail && (
        <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          {detail}
        </span>
      )}
    </div>
  );
}
