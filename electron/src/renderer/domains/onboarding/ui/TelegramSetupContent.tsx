import React, { useState, useCallback } from 'react';
import { useConfig } from '#renderer/domains/onboarding/runtime/hooks';
import { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
import { composePatch, envPatch, telegramPatch } from '#renderer/global/service/configPatches';
import { Input } from '#renderer/global/ui/input';
import { Button } from '#renderer/global/ui/button';
import { useTelegramTokenValidator } from '#renderer/domains/onboarding/runtime/useTelegramTokenValidator';

function StatusLine({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <span className={`text-[11px] leading-[14px] ${ok ? 'text-success' : 'text-destructive'}`}>
      {ok ? '\u2713' : '\u2717'} {label}
    </span>
  );
}

export function TelegramContent(): React.ReactElement {
  const { data: config } = useConfig();
  const { save, saveMut } = useConfigPatch();
  const existingToken = config?.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '';

  const [token, setToken] = useState(existingToken);
  const { validating, result, validate, reset: resetResult } = useTelegramTokenValidator();
  const saving = saveMut.isPending;

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

  const canSave = !!token.trim() && !validating && !saving;

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
          {saving ? 'Saving\u2026' : validating ? 'Validating\u2026' : 'Enable'}
        </Button>
      </div>
    </div>
  );
}
