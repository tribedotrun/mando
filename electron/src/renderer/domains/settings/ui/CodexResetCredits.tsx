import React from 'react';
import { useCodexResetCredits } from '#renderer/domains/settings/runtime/hooks';
import {
  formatResetCreditCount,
  formatWindowReset,
} from '#renderer/domains/settings/service/formatters';

interface CodexResetCreditsProps {
  credentialId: number;
  enabled: boolean;
}

export function CodexResetCredits({
  credentialId,
  enabled,
}: CodexResetCreditsProps): React.ReactElement | null {
  const creditsQuery = useCodexResetCredits(credentialId, enabled);
  if (!enabled) return null;
  if (creditsQuery.isLoading) {
    return (
      <div
        className="rounded-md bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground"
        data-testid="codex-reset-credits-loading"
      >
        Checking reset credits…
      </div>
    );
  }
  if (creditsQuery.isError) {
    return (
      <div
        className="rounded-md bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground"
        data-testid="codex-reset-credits-error"
      >
        Reset credits unavailable.
      </div>
    );
  }
  const credits = creditsQuery.data;
  if (!credits) return null;
  if (credits.availableCount <= 0) {
    return (
      <div
        className="rounded-md bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground"
        data-testid="codex-reset-credits-empty"
      >
        No reset credits available.
      </div>
    );
  }
  return (
    <div
      className="rounded-md bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground"
      data-testid="codex-reset-credits"
    >
      <div className="font-medium text-foreground">
        {formatResetCreditCount(credits.availableCount)}
      </div>
      <div className="mt-1 space-y-0.5">
        {credits.credits.map((credit) => (
          <div key={`${credit.title}-${credit.expiresAt}`}>
            <span className="text-foreground">{credit.title}</span>
            <span> · expires {formatWindowReset(credit.expiresAt)}</span>
            {credit.description ? <span> · {credit.description}</span> : null}
          </div>
        ))}
      </div>
    </div>
  );
}
