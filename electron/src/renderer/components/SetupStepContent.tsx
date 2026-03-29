import React, { useState, useCallback, useRef } from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useMountEffect } from '#renderer/hooks/useMountEffect';

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
          href="https://docs.anthropic.com/en/docs/claude-code/overview"
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
        window.mandoAPI.launchd.reinstall().catch(() => {});
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
  const updateProject = useSettingsStore((s) => s.updateProject);
  const save = useSettingsStore((s) => s.save);

  const handlePick = useCallback(async () => {
    const dir = await window.mandoAPI.selectDirectory();
    if (!dir) return;
    const name = dir.split('/').pop() ?? dir;
    updateProject(dir, { name, path: dir });
    save();
  }, [updateProject, save]);

  return (
    <div>
      <button onClick={handlePick} className={btnCls} style={primaryBtn}>
        Choose folder
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Linear — API key → auto-fetch teams → pick from list
// ---------------------------------------------------------------------------

export function LinearContent(): React.ReactElement {
  const existingKey = useSettingsStore((s) => s.config.env?.LINEAR_API_KEY ?? '');
  const existingTeam = useSettingsStore((s) => s.config.captain?.linearTeam ?? '');
  const updateSection = useSettingsStore((s) => s.updateSection);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);

  const [apiKey, setApiKey] = useState(existingKey);
  const [teams, setTeams] = useState<Array<{ id: string; key: string; name: string }>>([]);
  const [selectedTeam, setSelectedTeam] = useState(existingTeam);
  const [fetching, setFetching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useMountEffect(() => () => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
  });

  const fetchTeams = useCallback(async (key: string) => {
    setFetching(true);
    setError(null);
    setTeams([]);
    setSelectedTeam('');
    try {
      const res = await window.mandoAPI.validateLinearKey(key.trim());
      if (res.valid && res.teams.length > 0) {
        setTeams(res.teams);
        if (res.teams.length === 1) setSelectedTeam(res.teams[0].key);
      } else {
        setError(res.error ?? 'Invalid API key');
      }
    } catch {
      setError('Validation failed — try again');
    } finally {
      setFetching(false);
    }
  }, []);

  const handleApiKeyChange = useCallback(
    (value: string) => {
      setApiKey(value);
      setError(null);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (value.trim().length > 30) {
        debounceRef.current = setTimeout(() => fetchTeams(value), 600);
      }
    },
    [fetchTeams],
  );

  const canSave = !!selectedTeam && !!apiKey.trim() && teams.length > 0;

  const handleSave = useCallback(() => {
    updateEnv('LINEAR_API_KEY', apiKey.trim());
    updateSection('captain', { linearTeam: selectedTeam });
    updateSection('features', { linear: true });
    save();
  }, [selectedTeam, apiKey, updateEnv, updateSection, save]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        <input
          type="text"
          className={inputCls}
          style={inputStyle}
          value={apiKey}
          onChange={(e) => handleApiKeyChange(e.target.value)}
          placeholder="API key"
        />
        {fetching && (
          <span className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
            Fetching teams…
          </span>
        )}
        {error && <StatusLine ok={false} label={error} />}
        {teams.length > 0 && (
          <select
            className={inputCls}
            style={{ ...inputStyle, cursor: 'pointer' }}
            value={selectedTeam}
            onChange={(e) => setSelectedTeam(e.target.value)}
          >
            {teams.length > 1 && <option value="">Select a team</option>}
            {teams.map((t) => (
              <option key={t.id} value={t.key}>
                {t.name} ({t.key})
              </option>
            ))}
          </select>
        )}
      </div>
      <div className="flex items-center justify-between">
        <a
          href="https://linear.app/settings/api"
          target="_blank"
          rel="noopener noreferrer"
          className="text-[11px]"
          style={{ color: 'var(--color-text-3)' }}
        >
          Get API key
        </a>
        <button
          onClick={handleSave}
          disabled={!canSave}
          className={btnCls}
          style={{ ...primaryBtn, cursor: canSave ? 'pointer' : 'default' }}
        >
          Connect
        </button>
      </div>
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
