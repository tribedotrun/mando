import React, { useState, useCallback } from 'react';
import { useConfig } from '#renderer/domains/onboarding/runtime/hooks';
import { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
import { composePatch, envPatch, telegramPatch } from '#renderer/global/service/configPatches';
import { useAddProjectFromPicker } from '#renderer/global/runtime/useAddProjectFromPicker';
import { Input } from '#renderer/global/ui/input';
import { Button } from '#renderer/global/ui/button';
import { useTelegramTokenValidator } from '#renderer/domains/onboarding/runtime/useTelegramTokenValidator';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/service/types';

export type { ClaudeCheckResult };

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
  const { data: config } = useConfig();
  const { save } = useConfigPatch();
  const existingToken = config?.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '';

  const [token, setToken] = useState(existingToken);
  const { validating, result, validate, reset: resetResult } = useTelegramTokenValidator();

  const handleSave = useCallback(async () => {
    const ok = await validate(token);
    if (!ok) return;
    save(
      composePatch(
        envPatch({ TELEGRAM_MANDO_BOT_TOKEN: token.trim() }),
        telegramPatch({ enabled: true }),
      ),
    );
  }, [token, validate, save]);

  const canSave = !!token.trim() && !validating;

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs leading-4 text-muted-foreground">
        Create a bot via{' '}
        <a
          href="https://t.me/BotFather"
          target="_blank"
          rel="noopener noreferrer"
          className="text-foreground"
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
        <Button size="xs" onClick={() => void handleSave()} disabled={!canSave}>
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
  const { pickAndAdd, adding } = useAddProjectFromPicker();

  return (
    <div>
      <Button size="xs" onClick={() => void pickAndAdd()} disabled={adding}>
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
