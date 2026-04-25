import React from 'react';
import { RefreshCw } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Progress } from '#renderer/global/ui/primitives/progress';
import { cn } from '#renderer/global/service/cn';
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
      <span className="w-8 shrink-0 text-xs font-medium text-muted-foreground">{label}</span>
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
 * Per-credential 5h / 7d utilization bars with manual refresh.
 *
 * Data comes from the proactive usage probe (see
 * `rust/crates/settings/src/io/usage_probe.rs`). The poll runs in the background;
 * the refresh button fires an on-demand probe.
 */
export function CredentialUsage({ cred }: { cred: CredentialInfo }): React.ReactElement | null {
  const probeMut = useCredentialProbe();
  if (cred.isExpired) {
    return (
      <div className="mt-2 rounded-md border border-dashed border-destructive/40 px-3 py-2 text-xs text-destructive">
        Re-login required - run <code>claude setup-token</code> and re-add this credential.
      </div>
    );
  }
  const { fiveHour, sevenDay, lastProbedAt, costSinceProbeUsd } = cred;
  const sinceProbe = formatSinceProbe(lastProbedAt);
  if (fiveHour == null && sevenDay == null) {
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
      <div className="flex items-center justify-between pt-0.5 text-[11px] text-muted-foreground">
        <span>
          {sinceProbe ? `probed ${sinceProbe}` : ''}
          {costSinceProbeUsd != null && costSinceProbeUsd > 0
            ? ` · +${formatUsd(costSinceProbeUsd)} since`
            : ''}
        </span>
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
      </div>
    </div>
  );
}
