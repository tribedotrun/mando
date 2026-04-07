import React, { useState, useCallback } from 'react';
import { useSettingsStore } from '#renderer/domains/settings';
import { toast } from 'sonner';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';
import { Input } from '#renderer/components/ui/input';
import { Button } from '#renderer/components/ui/button';
import { useTelegramTokenValidator } from '#renderer/global/hooks/useTelegramTokenValidator';

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
    <div className="flex flex-col gap-2">
      <p className="text-xs leading-4 text-muted-foreground">
        Mando uses Claude Code to run AI agents. Install it, then verify below.
      </p>

      {checkResult?.checkFailed && (
        <StatusLine
          ok={false}
          label={`Check failed: ${checkResult.error ?? 'Unknown error'} \u2014 retry`}
        />
      )}

      {checkResult?.installed && !checkResult.checkFailed && (
        <div className="flex flex-col gap-1">
          {checkResult.version && (
            <span className="text-[11px] text-muted-foreground">{checkResult.version}</span>
          )}
          <StatusLine
            ok={checkResult.works}
            label={checkResult.works ? 'Responding' : 'Not responding \u2014 check your API key'}
          />
        </div>
      )}

      <div className="flex items-center gap-1.5">
        <Button size="xs" asChild>
          <a
            href="https://code.claude.com/docs/en/overview"
            target="_blank"
            rel="noopener noreferrer"
            className="no-underline"
          >
            Install Claude Code
          </a>
        </Button>
        <Button variant="outline" size="xs" onClick={recheckClaude} disabled={checking}>
          {checking ? 'Checking\u2026' : 'Check'}
        </Button>
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

  const handleSave = useCallback(async () => {
    const ok = await validate(token);
    if (!ok) return;
    updateEnv('TELEGRAM_MANDO_BOT_TOKEN', token.trim());
    updateTelegram({ enabled: true });
    await save();
  }, [token, validate, updateEnv, updateTelegram, save]);

  const canSave = !!token.trim() && !validating;

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs leading-4 text-muted-foreground">
        Create a bot via{' '}
        <a
          href="https://t.me/BotFather"
          target="_blank"
          rel="noopener noreferrer"
          className="text-primary"
        >
          @BotFather
        </a>{' '}
        and paste the token below.
      </p>
      <Input
        type="text"
        className="text-xs"
        value={token}
        onChange={(e) => {
          setToken(e.target.value);
          resetResult();
        }}
        placeholder="Bot token"
      />
      {result?.botUsername && <StatusLine ok label={`Connected \u2014 @${result.botUsername}`} />}
      {result?.error && <StatusLine ok={false} label={result.error} />}
      <div>
        <Button size="xs" onClick={handleSave} disabled={!canSave}>
          {validating ? 'Validating\u2026' : 'Enable'}
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Project
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
      <Button size="xs" onClick={handlePick} disabled={adding}>
        {adding ? 'Adding\u2026' : 'Choose folder'}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared status indicator
// ---------------------------------------------------------------------------

function StatusLine({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <span className={`text-[11px] leading-[14px] ${ok ? 'text-success' : 'text-destructive'}`}>
      {ok ? '\u2713' : '\u2717'} {label}
    </span>
  );
}
