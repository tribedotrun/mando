import React, { useState, useCallback } from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useToastStore } from '#renderer/stores/toastStore';

const inputCls =
  'w-full rounded px-2.5 py-1.5 text-xs placeholder-[var(--color-text-3)] focus:outline-none';
const inputStyle: React.CSSProperties = {
  border: '1px solid var(--color-border-subtle)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};
const btnCls = 'text-[11px] font-medium disabled:opacity-40';
const primaryBtn: React.CSSProperties = {
  padding: '5px 12px',
  borderRadius: 5,
  background: 'var(--color-accent)',
  color: 'var(--color-bg)',
  border: 'none',
  cursor: 'pointer',
};
const ghostBtn: React.CSSProperties = {
  padding: '5px 10px',
  borderRadius: 5,
  border: '1px solid var(--color-border-subtle)',
  background: 'transparent',
  color: 'var(--color-text-2)',
  cursor: 'pointer',
};

export interface ClaudeCheckResult {
  installed: boolean;
  version: string | null;
  works: boolean;
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
      <p className="text-xs" style={{ color: 'var(--color-text-2)', lineHeight: '16px' }}>
        Mando uses Claude Code to run AI agents. Install it, then verify below.
      </p>

      {checkResult?.installed && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {checkResult.version && (
            <span className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
              {checkResult.version}
            </span>
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
  const [validating, setValidating] = useState(false);
  const [result, setResult] = useState<{ botUsername?: string; error?: string } | null>(null);

  const handleSave = useCallback(async () => {
    setValidating(true);
    setResult(null);
    try {
      const res = await window.mandoAPI.validateTelegramToken(token.trim());
      if (res.valid) {
        setResult({ botUsername: res.botUsername });
        updateEnv('TELEGRAM_MANDO_BOT_TOKEN', token.trim());
        updateTelegram({ enabled: true });
        save();
        // Install TG launchd plist now that Telegram is configured
        window.mandoAPI.launchd.reinstall().catch(() => {
          useToastStore.getState().add('error', 'Failed to install Telegram service');
        });
      } else {
        setResult({ error: res.error ?? 'Invalid token' });
      }
    } catch {
      setResult({ error: 'Validation failed — try again' });
    } finally {
      setValidating(false);
    }
  }, [token, updateEnv, updateTelegram, save]);

  const canSave = !!token.trim() && !validating;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <p className="text-xs" style={{ color: 'var(--color-text-2)', lineHeight: '16px' }}>
        Create a bot via{' '}
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
      <input
        type="text"
        className={inputCls}
        style={inputStyle}
        value={token}
        onChange={(e) => {
          setToken(e.target.value);
          setResult(null);
        }}
        placeholder="Bot token"
      />
      {result?.botUsername && <StatusLine ok label={`Connected — @${result.botUsername}`} />}
      {result?.error && <StatusLine ok={false} label={result.error} />}
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
    const dir = await window.mandoAPI.selectDirectory();
    if (!dir) return;
    setAdding(true);
    try {
      await addProject({ name: '', path: dir });
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to add project';
      useToastStore.getState().add('error', msg);
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
      style={{ color: ok ? 'var(--color-success)' : 'var(--color-danger)', lineHeight: '14px' }}
    >
      {ok ? '✓' : '✗'} {label}
    </span>
  );
}
