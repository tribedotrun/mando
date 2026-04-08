import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import type { MandoConfig } from '#renderer/domains/settings';
import heroImg from '#renderer/assets/hero.png';
import { Button } from '#renderer/components/ui/button';
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
  'Your backlog runs itself \u2014 tasks get picked up, reviewed, and delivered as pull requests with visual evidence.',
  'Run tasks across multiple projects at once. Each gets its own isolated workspace.',
  'Stay current \u2014 relevant articles, repos, and podcasts become actionable tasks tailored to your stack.',
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
    void window.mandoAPI.saveConfigLocal(JSON.stringify(config, null, 2)).catch((e) => {
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
        void finishSetup();
      }}
      onSkip={() => {
        setTgToken('');
        void finishSetup('');
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
    <div data-testid="onboarding-wizard" className="relative flex h-full bg-background">
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <div className="flex w-[460px] shrink-0 flex-col justify-center p-12">
        <div className="mb-6">
          <h1 className="text-display text-foreground">Mando</h1>
          <p className="mt-2 text-subheading tracking-wide text-muted-foreground">
            Manage tasks, not agents.
          </p>
        </div>
        <ul className="m-0 flex list-none flex-col gap-6 p-0">
          {BULLETS.map((text, i) => (
            <li key={i} className="flex gap-3">
              <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-foreground" />
              <span className="text-body leading-relaxed text-foreground">{text}</span>
            </li>
          ))}
        </ul>
        <div className="pt-10">
          <Button data-testid="onboarding-next" onClick={onStart}>
            Get Started
          </Button>
        </div>
      </div>
      <div className="flex flex-1 items-center justify-center overflow-hidden pb-6 pl-0 pr-6 pt-6">
        <img
          src={heroImg}
          alt="Mando captain view"
          className="h-auto w-full rounded-lg object-contain shadow-[0_8px_32px_rgba(0,0,0,0.35)]"
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
    void runCheck();
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
      <div className="mb-10 flex flex-col gap-3 rounded-lg bg-muted px-7 py-7 shadow-sm">
        {checking && (
          <div className="flex items-center gap-2.5">
            <span className="size-3.5 animate-spin shrink-0 rounded-full border-2 border-foreground border-t-transparent" />
            <span className="text-body text-muted-foreground">Checking...</span>
          </div>
        )}
        {result && !checking && result.checkFailed && (
          <StatusCheck
            ok={false}
            label={`Check failed: ${result.error ?? 'Unknown error'} \u2014 retry`}
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
        <p className="mb-6 text-body text-muted-foreground">
          Install Claude Code from{' '}
          <a
            href="https://code.claude.com/docs/en/overview"
            target="_blank"
            rel="noopener noreferrer"
            className="text-foreground"
          >
            the docs
          </a>
          , then re-check.
        </p>
      )}

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <GhostButton onClick={onBack}>Back</GhostButton>
          {!passed && (
            <OutlineButton onClick={() => void runCheck()} disabled={checking}>
              {checking ? 'Checking...' : 'Re-check'}
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
    <div className="flex items-center gap-2.5">
      <span
        className={`flex size-4 shrink-0 items-center justify-center rounded-full text-[11px] font-bold text-background ${ok ? 'bg-success' : 'bg-destructive'}`}
      >
        {ok ? '\u2713' : '\u2717'}
      </span>
      <span className={`text-body ${ok ? 'text-foreground' : 'text-destructive'}`}>{label}</span>
      {detail && <span className="text-caption text-muted-foreground">{detail}</span>}
    </div>
  );
}
