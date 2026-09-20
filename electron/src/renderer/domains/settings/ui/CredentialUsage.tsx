import React from 'react';
import { RefreshCw } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Progress } from '#renderer/global/ui/primitives/progress';
import { cn } from '#renderer/global/service/cn';
import { CodexResetCredits } from '#renderer/domains/settings/ui/CodexResetCredits';
import { CodexWarmupButton } from '#renderer/domains/settings/ui/CodexWarmupButton';
import { CredentialExpiredNotice } from '#renderer/domains/settings/ui/CredentialExpiredNotice';
import {
  useCredentialProbe,
  type CredentialInfo,
  type CredentialWindowInfo,
  type CredentialRateLimitStatus,
} from '#renderer/domains/settings/runtime/hooks';
import {
  formatSinceProbe,
  formatUsd,
  formatUtilizationPct,
  formatWindowReset,
  utilizationToBarValue,
} from '#renderer/domains/settings/service/formatters';

const STATUS_BAR_CLASSES: Record<CredentialRateLimitStatus, string> = Object.freeze({
  allowed: '[&_[data-slot=progress-indicator]]:bg-success',
  allowed_warning: '[&_[data-slot=progress-indicator]]:bg-warning',
  rejected: '[&_[data-slot=progress-indicator]]:bg-destructive',
});

function WindowRow({
  label,
  window: w,
}: {
  label: string;
  window: CredentialWindowInfo;
}): React.ReactElement {
  const pct = utilizationToBarValue(w.utilization);
  return (
    <div className="flex items-center gap-3" data-testid={`credential-window-${label}`}>
      <span className="w-16 shrink-0 text-xs font-medium text-muted-foreground">{label}</span>
      <Progress value={pct} className={cn('h-1.5 flex-1', STATUS_BAR_CLASSES[w.status])} />
      <span className="w-10 shrink-0 text-right text-xs tabular-nums text-foreground">
        {formatUtilizationPct(w.utilization)}
      </span>
      <span className="w-52 shrink-0 whitespace-nowrap text-right text-xs text-muted-foreground">
        resets {formatWindowReset(w.resetAt)}
      </span>
    </div>
  );
}

/**
 * Per-credential usage windows, including Fable's weekly limit, with manual refresh.
 *
 * Data comes from the proactive usage probe (see
 * `rust/crates/settings/src/io/usage_probe.rs`). The poll runs in the background;
 * the refresh button fires an on-demand probe.
 */
export function CredentialUsage({ cred }: { cred: CredentialInfo }): React.ReactElement | null {
  const probeMut = useCredentialProbe();
  if (cred.isExpired) {
    return <CredentialExpiredNotice cred={cred} />;
  }
  const { fiveHour, sevenDay, sevenDayFable, lastProbedAt, costSinceProbeUsd } = cred;
  const sinceProbe = formatSinceProbe(lastProbedAt);
  const sinceWarmup = formatSinceProbe(cred.codex?.warmupAt);
  if (fiveHour == null && sevenDay == null && sevenDayFable == null) {
    return (
      <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
        <span>Usage not yet probed.</span>
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={() => probeMut.mutate(cred.id)}
          disabled={probeMut.isPending}
          title="Probe now"
          aria-label="Probe credential usage"
        >
          <RefreshCw size={12} className={probeMut.isPending ? 'animate-spin' : undefined} />
        </Button>
      </div>
    );
  }
  return (
    <div className="mt-2 space-y-1.5" data-testid="credential-usage">
      {fiveHour ? <WindowRow label="5h" window={fiveHour} /> : null}
      {sevenDay ? <WindowRow label="7d" window={sevenDay} /> : null}
      {cred.provider === 'claude' && sevenDayFable ? (
        <WindowRow label="Fable 7d" window={sevenDayFable} />
      ) : null}
      {cred.provider === 'codex' ? (
        <CodexResetCredits credentialId={cred.id} enabled={!cred.isExpired} />
      ) : null}
      <div className="flex items-center justify-between pt-0.5 text-[11px] text-muted-foreground">
        <span>
          {sinceProbe ? `probed ${sinceProbe}` : ''}
          {costSinceProbeUsd != null && costSinceProbeUsd > 0
            ? ` · +${formatUsd(costSinceProbeUsd)} since`
            : ''}
          {cred.provider === 'codex' && sinceWarmup ? (
            <span data-testid="codex-warmup-since"> · clock started {sinceWarmup}</span>
          ) : null}
        </span>
        <span className="flex items-center gap-0.5">
          {cred.provider === 'codex' ? (
            <CodexWarmupButton credentialId={cred.id} disabled={cred.isDisabled} />
          ) : null}
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => probeMut.mutate(cred.id)}
            disabled={probeMut.isPending}
            title="Refresh usage"
            aria-label="Refresh credential usage"
          >
            <RefreshCw size={12} className={probeMut.isPending ? 'animate-spin' : undefined} />
          </Button>
        </span>
      </div>
    </div>
  );
}
