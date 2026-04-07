import React, { useState, useCallback } from 'react';
import { useSettingsStore } from '#renderer/domains/settings';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { inputClsCompact, inputStyleSubtle } from '#renderer/styles';
import { useTelegramTokenValidator } from '#renderer/global/hooks/useTelegramTokenValidator';
import log from '#renderer/logger';

const btnCls = 'text-[11px] font-medium disabled:opacity-40';
const primaryBtn: React.CSSProperties = {
  padding: '8px 20px',
  borderRadius: 6,
  background: 'var(--color-accent)',
  color: 'var(--color-bg)',
  border: 'none',
  cursor: 'pointer',
};
const ghostBtn: React.CSSProperties = {
  padding: '8px 20px',
  borderRadius: 6,
  border: '1px solid var(--color-border)',
  background: 'transparent',
  color: 'var(--color-text-2)',
  cursor: 'pointer',
};

export interface ClaudeCheckResult {
  installed: boolean;
  version: string | null;
  works: boolean;
  checkFailed?: boolean;
  error?: string;
}

export function ClaudeCodeContent({
  recheckClaude,
  checkResult,
}: {
  recheckClaude: () => void;
  checkResult: ClaudeCheckResult | null;
}): React.ReactElement {
  // Derive checking state: true after user clicks Check until result arrives
  const checking = checkResult === null;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <p className="text-xs text-text-2" style={{ lineHeight: '16px' }}>
        Mando uses Claude Code to run AI agents. Install it, then verify below.
      </p>

      {checkResult?.checkFailed && (
        <StatusLine
          ok={false}
          label={`Check failed: ${checkResult.error ?? 'Unknown error'} — retry`}
        />
      )}

      {checkResult?.installed && !checkResult.checkFailed && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {checkResult.version && (
            <span className="text-[11px] text-text-3">{checkResult.version}</span>
          )}
          <StatusLine
            ok={checkResult.works}
            label={checkResult.works ? 'Responding' : 'Not responding — check your API key'}
          />
        </div>
      )}

      <div className="flex items-center" style={{ gap: 6 }}>
        <a
          href="https://code.claude.com/docs/en/overview"
          target="_blank"
          rel="noopener noreferrer"
          className={btnCls}
          style={{ ...primaryBtn, textDecoration: 'none', display: 'inline-block' }}
        >
          Install Claude Code
        </a>
        <button onClick={recheckClaude} disabled={checking} className={btnCls} style={ghostBtn}>
          {checking ? 'Checking…' : 'Check'}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Telegram
// ---------------------------------------------------------------------------

export function TelegramContent(): React.ReactElement {
  const existingToken = useSettingsStore((s) => s.config.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '');
  const updateTelegram = useSettingsStore((s) => s.updateTelegram);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);

  const [token, setToken] = useState(existingToken);
  const { validating, result, validate, reset: resetResult } = useTelegramTokenValidator();
  const [serviceWarning, setServiceWarning] = useState<string | null>(null);

  const handleSave = useCallback(async () => {
    setServiceWarning(null);
    const ok = await validate(token);
    if (!ok) return;
    updateEnv('TELEGRAM_MANDO_BOT_TOKEN', token.trim());
    updateTelegram({ enabled: true });
    save();
    // Install TG launchd plist now that Telegram is configured.
    try {
      await window.mandoAPI.launchd.reinstall();
    } catch (reinstallErr) {
      log.warn('[Setup] TG launchd reinstall failed', reinstallErr);
      setServiceWarning('Service install pending, restart Mando');
      toast.error(getErrorMessage(reinstallErr, 'Failed to install Telegram service'));
    }
  }, [token, validate, updateEnv, updateTelegram, save]);

  const canSave = !!token.trim() && !validating;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <p className="text-xs text-text-2" style={{ lineHeight: '16px' }}>
        Create a bot via{' '}
        <a
          href="https://t.me/BotFather"
          target="_blank"
          rel="noopener noreferrer"
          className="text-accent"
        >
          @BotFather
        </a>{' '}
        and paste the token below.
      </p>
      <input
        type="text"
        className={inputClsCompact}
        style={inputStyleSubtle}
        value={token}
        onChange={(e) => {
          setToken(e.target.value);
          resetResult();
        }}
        placeholder="Bot token"
      />
      {result?.botUsername && <StatusLine ok label={`Connected — @${result.botUsername}`} />}
      {result?.error && <StatusLine ok={false} label={result.error} />}
      {serviceWarning && <StatusLine ok={false} label={serviceWarning} />}
      <div>
        <button
          onClick={handleSave}
          disabled={!canSave}
          className={btnCls}
          style={{ ...primaryBtn, cursor: canSave ? 'pointer' : 'default' }}
        >
          {validating ? 'Validating…' : 'Enable'}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Project — minimal inline form to add a project path
// ---------------------------------------------------------------------------

export function ProjectContent(): React.ReactElement {
  const addProject = useSettingsStore((s) => s.addProject);
  const [adding, setAdding] = useState(false);

  const handlePick = useCallback(async () => {
    let dir: string | null;
    try {
      dir = await window.mandoAPI.selectDirectory();
    } catch (err) {
      log.warn('[Setup] selectDirectory failed', err);
      toast.error(getErrorMessage(err, 'Failed to open folder picker'));
      return;
    }
    if (!dir) return;
    setAdding(true);
    try {
      await addProject({ name: '', path: dir });
    } catch (err) {
      log.warn('[Setup] addProject failed', err);
      toast.error(getErrorMessage(err, 'Failed to add project'));
    } finally {
      setAdding(false);
    }
  }, [addProject]);

  return (
    <div>
      <button onClick={handlePick} disabled={adding} className={btnCls} style={primaryBtn}>
        {adding ? 'Adding…' : 'Choose folder'}
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared status indicator
// ---------------------------------------------------------------------------

function StatusLine({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <span
      className="text-[11px]"
      style={{ color: ok ? 'var(--color-success)' : 'var(--color-error)', lineHeight: '14px' }}
    >
      {ok ? '✓' : '✗'} {label}
    </span>
  );
}
