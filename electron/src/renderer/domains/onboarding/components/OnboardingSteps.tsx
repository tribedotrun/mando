import React, { useCallback } from 'react';
import { Input } from '#renderer/components/ui/input';
import {
  SetupLayout,
  CheckRow,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/domains/onboarding/components/OnboardingPrimitives';
import { useTelegramTokenValidator } from '#renderer/global/hooks/useTelegramTokenValidator';

// ---- Telegram setup ----

export function TelegramScreen({
  token,
  onTokenChange,
  onBack,
  onNext,
  onSkip,
  error,
  finishing,
  progressMsg,
}: {
  token: string;
  onTokenChange: (v: string) => void;
  onBack: () => void;
  onNext: () => void;
  onSkip: () => void;
  error?: string | null;
  finishing?: boolean;
  progressMsg?: string | null;
}): React.ReactElement {
  const {
    validating,
    result: tgResult,
    validate: runValidate,
    reset,
  } = useTelegramTokenValidator();
  const validate = useCallback(async () => {
    await runValidate(token);
  }, [token, runValidate]);

  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      step={2}
      total={2}
      title="Telegram"
      subtitle="Notifications and remote control from your phone."
    >
      <div className="mb-10 rounded-lg bg-muted px-7 py-7 shadow-sm">
        <p className="mb-6 text-caption leading-relaxed text-muted-foreground">
          Open{' '}
          <a
            href="https://t.me/BotFather"
            target="_blank"
            rel="noopener noreferrer"
            className="text-foreground"
          >
            @BotFather
          </a>{' '}
          in Telegram and send <code className="text-muted-foreground">/newbot</code>. Give it a
          display name and a username ending in &ldquo;bot&rdquo;. Copy the token.
        </p>
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <Input
              className="flex-1 text-[13px]"
              value={token}
              onChange={(e) => {
                onTokenChange(e.target.value);
                reset();
              }}
              placeholder="Bot token"
            />
            <OutlineButton onClick={() => void validate()} disabled={!token.trim() || validating}>
              <span className="inline-block min-w-[52px] text-center">
                {validating ? 'Connecting\u2026' : 'Connect'}
              </span>
            </OutlineButton>
          </div>
          {tgResult?.botUsername && <CheckRow ok label={`@${tgResult.botUsername}`} />}
          {tgResult?.error && <CheckRow ok={false} label={tgResult.error} />}
        </div>
      </div>

      {error && (
        <div className="mb-4 rounded-sm bg-destructive/10 px-3 py-1.5 text-caption text-destructive">
          {error}
        </div>
      )}

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          {!finishing && <GhostButton onClick={onBack}>Back</GhostButton>}
          {!tgResult?.botUsername && !finishing && <GhostButton onClick={onSkip}>Skip</GhostButton>}
        </div>
        <div className="flex items-center gap-3">
          {finishing && progressMsg && (
            <span className="text-caption text-muted-foreground">{progressMsg}</span>
          )}
          <PrimaryButton onClick={onNext} disabled={finishing || !tgResult?.botUsername}>
            {finishing ? 'Setting up\u2026' : 'Finish Setup'}
          </PrimaryButton>
        </div>
      </div>
    </SetupLayout>
  );
}
