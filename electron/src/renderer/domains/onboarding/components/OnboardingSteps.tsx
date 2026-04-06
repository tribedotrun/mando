import React, { useCallback } from 'react';
import {
  SetupLayout,
  CheckRow,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/domains/onboarding/components/OnboardingPrimitives';
import { inputClsCompact, inputStyleSubtleFlex } from '#renderer/styles';
import { useTelegramTokenValidator } from '#renderer/global/hooks/useTelegramTokenValidator';

const FORM_CARD: React.CSSProperties = {
  padding: '28px 28px',
  borderRadius: 'var(--radius-panel)',
  background: 'var(--color-surface-2)',
  border: '1px solid var(--color-border-subtle)',
  boxShadow: '0 1px 4px rgba(0, 0, 0, 0.2)',
  marginBottom: 40,
};

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
      <div style={FORM_CARD}>
        <p
          className="text-caption"
          style={{ color: 'var(--color-text-3)', marginBottom: 24, lineHeight: 1.5 }}
        >
          Open{' '}
          <a
            href="https://t.me/BotFather"
            target="_blank"
            rel="noopener noreferrer"
            style={{ color: 'var(--color-accent)' }}
          >
            @BotFather
          </a>{' '}
          in Telegram and send <code style={{ color: 'var(--color-text-2)' }}>/newbot</code>. Give
          it a display name and a username ending in &ldquo;bot&rdquo;. Copy the token.
        </p>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div className="flex items-center" style={{ gap: 8 }}>
            <input
              className={inputClsCompact}
              style={{ ...inputStyleSubtleFlex, fontSize: 13 }}
              value={token}
              onChange={(e) => {
                onTokenChange(e.target.value);
                reset();
              }}
              placeholder="Bot token"
            />
            <OutlineButton onClick={validate} disabled={!token.trim() || validating}>
              <span style={{ display: 'inline-block', minWidth: 52, textAlign: 'center' }}>
                {validating ? 'Connecting\u2026' : 'Connect'}
              </span>
            </OutlineButton>
          </div>
          {tgResult?.botUsername && <CheckRow ok label={`@${tgResult.botUsername}`} />}
          {tgResult?.error && <CheckRow ok={false} label={tgResult.error} />}
        </div>
      </div>

      {error && (
        <div
          className="text-caption"
          style={{
            marginBottom: 16,
            padding: '6px 12px',
            borderRadius: 'var(--radius-row)',
            background: 'var(--color-error-bg)',
            color: 'var(--color-error)',
          }}
        >
          {error}
        </div>
      )}

      <div className="flex items-center" style={{ justifyContent: 'space-between' }}>
        <div className="flex items-center" style={{ gap: 12 }}>
          {!finishing && <GhostButton onClick={onBack}>Back</GhostButton>}
          {!tgResult?.botUsername && !finishing && <GhostButton onClick={onSkip}>Skip</GhostButton>}
        </div>
        <div className="flex items-center" style={{ gap: 12 }}>
          {finishing && progressMsg && (
            <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
              {progressMsg}
            </span>
          )}
          <PrimaryButton onClick={onNext} disabled={finishing || !tgResult?.botUsername}>
            {finishing ? 'Setting up\u2026' : 'Finish Setup'}
          </PrimaryButton>
        </div>
      </div>
    </SetupLayout>
  );
}
