import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/service/types';

function StatusLine({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <span className={`text-[11px] leading-[14px] ${ok ? 'text-success' : 'text-destructive'}`}>
      {ok ? '✓' : '✗'} {label}
    </span>
  );
}

export function ClaudeCodeContent({
  recheckClaude,
  checkResult,
}: {
  recheckClaude: () => void;
  checkResult: ClaudeCheckResult | null;
}): React.ReactElement {
  const checking = checkResult === null;

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs leading-4 text-muted-foreground">
        Mando uses Claude Code to run AI agents. Install it, then verify below.
      </p>

      {checkResult?.checkFailed && (
        <StatusLine
          ok={false}
          label={`Check failed: ${checkResult.error ?? 'Unknown error'} — retry`}
        />
      )}

      {checkResult?.installed && !checkResult.checkFailed && (
        <div className="flex flex-col gap-1">
          {checkResult.version && (
            <span className="text-[11px] text-muted-foreground">{checkResult.version}</span>
          )}
          <StatusLine
            ok={checkResult.works}
            label={checkResult.works ? 'Responding' : 'Not responding — check your API key'}
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
          {checking ? 'Checking…' : 'Check'}
        </Button>
      </div>
    </div>
  );
}
