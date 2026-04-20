import React from 'react';
import { useAddProjectFromPicker } from '#renderer/domains/onboarding/runtime/useAddProjectFromPicker';
import { Button } from '#renderer/global/ui/button';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/service/types';

export type { ClaudeCheckResult };
export { TelegramContent } from '#renderer/domains/onboarding/ui/TelegramSetupContent';

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

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

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
