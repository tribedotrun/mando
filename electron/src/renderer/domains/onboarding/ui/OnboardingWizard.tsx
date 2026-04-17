import React, { useState, useCallback } from 'react';
import { useSetupIpc } from '#renderer/domains/onboarding/runtime/useSetupIpc';
import heroImg from '#renderer/assets/hero.png';
import { Button } from '#renderer/global/ui/button';
import { TelegramScreen } from '#renderer/domains/onboarding/ui/OnboardingSteps';
import { ClaudeCheckScreen } from '#renderer/domains/onboarding/ui/ClaudeCheckScreen';
import { formatSetupError } from '#renderer/domains/onboarding/service/types';
import { toast } from 'sonner';
import log from '#renderer/global/service/logger';
import { getErrorMessage } from '#renderer/global/service/utils';

type Step = 'welcome' | 'claude-check' | 'telegram' | 'finishing';

const BULLETS = [
  'Your backlog runs itself \u2014 tasks get picked up, reviewed, and delivered as pull requests with visual evidence.',
  'Run tasks across multiple projects at once. Each gets its own isolated workspace.',
  'Stay current \u2014 relevant articles, repos, and podcasts become actionable tasks tailored to your stack.',
];

export function OnboardingWizard(): React.ReactElement {
  const { progressMsg, saveProgress, completeSetup, checkClaudeCode } = useSetupIpc();
  const [step, setStep] = useState<Step>('welcome');
  const [error, setError] = useState<string | null>(null);
  const [tgToken, setTgToken] = useState('');

  const handleSaveProgress = useCallback(() => {
    void saveProgress(tgToken).catch((e) => {
      log.error('Failed to save onboarding progress:', e);
      toast.error(getErrorMessage(e, 'Failed to save onboarding progress'));
    });
  }, [tgToken, saveProgress]);

  const finishSetup = useCallback(
    async (tokenOverride?: string) => {
      setError(null);
      setStep('finishing');
      try {
        const result = await completeSetup(tokenOverride ?? tgToken);
        if (!result.ok) {
          log.error('[Onboarding] setup-complete partial failure:', result);
          setError(formatSetupError(result));
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
    [tgToken, completeSetup],
  );

  if (step === 'welcome') {
    return <WelcomeScreen onStart={() => setStep('claude-check')} />;
  }

  if (step === 'claude-check') {
    return (
      <ClaudeCheckScreen
        onBack={() => setStep('welcome')}
        onPass={() => setStep('telegram')}
        checkClaudeCode={checkClaudeCode}
      />
    );
  }

  return (
    <TelegramScreen
      token={tgToken}
      onTokenChange={setTgToken}
      onBack={() => setStep('claude-check')}
      onNext={() => {
        handleSaveProgress();
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
          className="h-auto w-full rounded-lg object-contain shadow-[0_8px_32px_color-mix(in_srgb,black_35%,transparent)]"
        />
      </div>
    </div>
  );
}
