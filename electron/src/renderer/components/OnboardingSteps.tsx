import React, { useState, useCallback } from 'react';
import {
  INPUT_CLS,
  INPUT_STYLE,
  SetupLayout,
  CheckRow,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/components/OnboardingPrimitives';

// ---- Shared types ----

type TGResult = { botUsername?: string; error?: string } | null;
type LinearTeam = { id: string; key: string; name: string };
type LinearResult = { teams?: LinearTeam[]; error?: string } | null;

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
}: {
  token: string;
  onTokenChange: (v: string) => void;
  onBack: () => void;
  onNext: () => void;
  onSkip: () => void;
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

  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      step={2}
      total={3}
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
              className={INPUT_CLS}
              style={{ ...INPUT_STYLE, fontSize: 13 }}
              value={token}
              onChange={(e) => {
                onTokenChange(e.target.value);
                setTgResult(null);
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

      <div className="flex items-center" style={{ justifyContent: 'space-between' }}>
        <div className="flex items-center" style={{ gap: 12 }}>
          <GhostButton onClick={onBack}>Back</GhostButton>
          {!tgResult?.botUsername && <GhostButton onClick={onSkip}>Skip</GhostButton>}
        </div>
        <PrimaryButton onClick={onNext} disabled={!tgResult?.botUsername}>
          Continue
        </PrimaryButton>
      </div>
    </SetupLayout>
  );
}

// ---- Linear setup (optional) ----

export function LinearScreen({
  apiKey,
  onApiKeyChange,
  selectedTeam,
  onTeamChange,
  onBack,
  onFinish,
  error,
  finishing,
  progressMsg,
}: {
  apiKey: string;
  onApiKeyChange: (v: string) => void;
  selectedTeam: string;
  onTeamChange: (v: string) => void;
  onBack: () => void;
  onFinish: () => void;
  error: string | null;
  finishing: boolean;
  progressMsg: string | null;
}): React.ReactElement {
  const [validating, setValidating] = useState(false);
  const [result, setResult] = useState<LinearResult>(null);

  const validate = useCallback(async () => {
    setValidating(true);
    setResult(null);
    try {
      const res = await window.mandoAPI.validateLinearKey(apiKey.trim());
      if (res.valid && res.teams?.length) {
        setResult({ teams: res.teams });
        onTeamChange(res.teams[0].key);
      } else {
        setResult({ error: res.error ?? 'Invalid API key' });
      }
    } catch {
      setResult({ error: 'Validation failed' });
    } finally {
      setValidating(false);
    }
  }, [apiKey, selectedTeam, onTeamChange]);

  const hasTeams = !!result?.teams?.length;

  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      step={3}
      total={3}
      title="Linear"
      subtitle="Sync tasks with Linear."
    >
      <div style={FORM_CARD}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div className="flex items-center" style={{ gap: 8 }}>
            <input
              className={INPUT_CLS}
              style={{ ...INPUT_STYLE, fontSize: 13 }}
              value={apiKey}
              onChange={(e) => {
                onApiKeyChange(e.target.value);
                setResult(null);
                onTeamChange('');
              }}
              placeholder="API key"
            />
            <OutlineButton onClick={validate} disabled={!apiKey.trim() || validating}>
              <span style={{ display: 'inline-block', minWidth: 52, textAlign: 'center' }}>
                {validating ? 'Connecting\u2026' : 'Connect'}
              </span>
            </OutlineButton>
          </div>
          <a
            href="https://linear.app/settings/api"
            target="_blank"
            rel="noopener noreferrer"
            className="text-caption"
            style={{ color: 'var(--color-text-3)' }}
          >
            linear.app/settings/api
          </a>
          {hasTeams && (
            <div className="flex flex-col" style={{ gap: 4, marginTop: 4 }}>
              <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                Team
              </span>
              <select
                className={INPUT_CLS}
                style={{ ...INPUT_STYLE, fontSize: 13, flex: 'none', width: 'auto' }}
                value={selectedTeam}
                onChange={(e) => onTeamChange(e.target.value)}
              >
                {result.teams!.map((t) => (
                  <option key={t.key} value={t.key}>
                    {t.name} ({t.key})
                  </option>
                ))}
              </select>
            </div>
          )}
          {result?.error && <CheckRow ok={false} label={result.error} />}
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
        {!finishing && <GhostButton onClick={onBack}>Back</GhostButton>}
        <div className="flex items-center" style={{ gap: 12 }}>
          {finishing && progressMsg && (
            <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
              {progressMsg}
            </span>
          )}
          <PrimaryButton onClick={onFinish} disabled={finishing}>
            {finishing ? 'Setting up\u2026' : 'Finish Setup'}
          </PrimaryButton>
        </div>
      </div>
    </SetupLayout>
  );
}
