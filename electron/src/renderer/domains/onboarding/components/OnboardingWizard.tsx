import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import type { MandoConfig } from '#renderer/domains/settings';
import heroImg from '#renderer/assets/hero.png';
import {
  SetupLayout,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/domains/onboarding/components/OnboardingPrimitives';
import { TelegramScreen } from '#renderer/domains/onboarding/components/OnboardingSteps';
import { toast } from 'sonner';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';

type Step = 'welcome' | 'claude-check' | 'telegram' | 'finishing';

type CCResult = {
  installed: boolean;
  version: string | null;
  works: boolean;
  checkFailed?: boolean;
  error?: string;
} | null;

const BULLETS = [
  'Your backlog runs itself — tasks get picked up, reviewed, and delivered as pull requests with visual evidence.',
  'Run tasks across multiple projects at once. Each gets its own isolated workspace.',
  'Stay current — relevant articles, repos, and podcasts become actionable tasks tailored to your stack.',
];

export function OnboardingWizard(): React.ReactElement {
  const [step, setStep] = useState<Step>('welcome');
  const [error, setError] = useState<string | null>(null);
  const [tgToken, setTgToken] = useState('');
  const [progressMsg, setProgressMsg] = useState<string | null>(null);

  useMountEffect(() => {
    window.mandoAPI.onSetupProgress(setProgressMsg);
  });

  /** Persist partial config to disk so progress survives a crash or quit. */
  const saveProgress = useCallback(() => {
    const config: MandoConfig = { features: { claudeCodeVerified: true } };
    const env: Record<string, string> = {};
    if (tgToken.trim()) {
      config.channels = { telegram: { enabled: true } };
      env.TELEGRAM_MANDO_BOT_TOKEN = tgToken.trim();
    }
    if (Object.keys(env).length > 0) config.env = env;
    window.mandoAPI.saveConfigLocal(JSON.stringify(config, null, 2)).catch((e) => {
      log.error('Failed to save onboarding progress:', e);
      toast.error(getErrorMessage(e, 'Failed to save onboarding progress'));
    });
  }, [tgToken]);

  const finishSetup = useCallback(
    async (tokenOverride?: string) => {
      setError(null);
      setStep('finishing');
      try {
        const effectiveToken = tokenOverride ?? tgToken;
        const config: MandoConfig = {
          features: { claudeCodeVerified: true },
          captain: { autoSchedule: true },
        };
        const env: Record<string, string> = {};
        if (effectiveToken.trim()) {
          config.channels = { telegram: { enabled: true } };
          env.TELEGRAM_MANDO_BOT_TOKEN = effectiveToken.trim();
        }
        if (Object.keys(env).length > 0) config.env = env;
        const result = await window.mandoAPI.setupComplete(JSON.stringify(config, null, 2));
        if (!result.ok) {
          log.error('[Onboarding] setup-complete partial failure:', result);
          const parts: string[] = [];
          if (!result.daemonNotified) parts.push('daemon did not respond');
          if (!result.launchdInstalled) parts.push('background service install failed');
          const detail = parts.join(', ');
          const suffix = result.error ? `: ${result.error}` : '';
          setError(
            `Setup partially failed (${detail})${suffix}. You can continue, but restart Mando if things feel stuck.`,
          );
          setStep('telegram');
          return;
        }
        window.location.reload();
      } catch (err) {
        log.error('[Onboarding] setup-complete failed:', err);
        setError(getErrorMessage(err, 'Failed to save configuration'));
        setStep('telegram');
      }
    },
    [tgToken],
  );

  if (step === 'welcome') {
    return <WelcomeScreen onStart={() => setStep('claude-check')} />;
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
      onNext={() => {
        saveProgress();
        finishSetup();
      }}
      onSkip={() => {
        setTgToken('');
        finishSetup('');
      }}
      error={error}
      finishing={step === 'finishing'}
      progressMsg={progressMsg}
    />
  );
}

// ---- Welcome screen ----

function WelcomeScreen({ onStart }: { onStart: () => void }): React.ReactElement {
  return (
    <div data-testid="onboarding-wizard" className="relative flex h-full bg-bg">
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <div
        className="flex flex-col justify-center"
        style={{ width: 460, padding: 48, flexShrink: 0 }}
      >
        <div style={{ marginBottom: 24 }}>
          <h1 className="text-display text-text-1">Mando</h1>
          <p
            className="text-subheading text-text-2"
            style={{ marginTop: 8, letterSpacing: '0.01em' }}
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
                  borderRadius: 4,
                  background: 'var(--color-accent)',
                  flexShrink: 0,
                  marginTop: 6,
                }}
              />
              <span className="text-body text-text-1" style={{ lineHeight: 1.5 }}>
                {text}
              </span>
            </li>
          ))}
        </ul>
        <div style={{ paddingTop: 40 }}>
          <button data-testid="onboarding-next" onClick={onStart} className="btn btn-primary">
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
    } catch (err) {
      log.error('checkClaudeCode failed:', err);
      setResult({
        installed: false,
        version: null,
        works: false,
        checkFailed: true,
        error: getErrorMessage(err, 'Unknown error'),
      });
    } finally {
      setChecking(false);
    }
  }, []);

  useMountEffect(() => {
    runCheck();
  });

  const passed = result?.installed === true && result.works === true;

  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      step={1}
      total={2}
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
                borderRadius: 8,
                border: '2px solid var(--color-accent)',
                borderTopColor: 'transparent',
                flexShrink: 0,
              }}
            />
            <span className="text-body text-text-3">Checking…</span>
          </div>
        )}
        {result && !checking && result.checkFailed && (
          <StatusCheck
            ok={false}
            label={`Check failed: ${result.error ?? 'Unknown error'} — retry`}
          />
        )}
        {result && !checking && !result.checkFailed && (
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

      {result && !result.checkFailed && !result.installed && !checking && (
        <p className="text-body text-text-2" style={{ marginBottom: 24 }}>
          Install Claude Code from{' '}
          <a
            href="https://code.claude.com/docs/en/overview"
            target="_blank"
            rel="noopener noreferrer"
            className="text-accent"
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
          background: ok ? 'var(--color-success)' : 'var(--color-error)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0,
          fontSize: 11,
          color: 'var(--color-bg)',
          fontWeight: 700,
        }}
      >
        {ok ? '✓' : '✗'}
      </span>
      <span
        className="text-body"
        style={{ color: ok ? 'var(--color-text-1)' : 'var(--color-error)' }}
      >
        {label}
      </span>
      {detail && <span className="text-caption text-text-3">{detail}</span>}
    </div>
  );
}
