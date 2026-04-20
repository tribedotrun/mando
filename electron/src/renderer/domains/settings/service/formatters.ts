export function formatExpiry(expiresAt: number | null): string | null {
  if (expiresAt == null) return null;
  const now = Date.now();
  const diff = expiresAt - now;
  if (diff <= 0) return 'Expired';
  const hours = (diff / (1000 * 60 * 60)) | 0;
  if (hours < 24) return `${hours}h remaining`;
  const days = (hours / 24) | 0;
  return `${days}d remaining`;
}

/** Rate-limit window reset time as a human string: `14:22 (in 1h 23m)`. */
export function formatWindowReset(resetAtSecs: number | null | undefined): string {
  if (resetAtSecs == null) return '--';
  const resetMs = resetAtSecs * 1000;
  const diffMs = resetMs - Date.now();
  if (diffMs <= 0) return 'now';
  const at = new Date(resetMs);
  const timeStr = at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const totalMin = Math.round(diffMs / 60_000);
  const days = Math.floor(totalMin / 1440);
  const hours = Math.floor((totalMin % 1440) / 60);
  const mins = totalMin % 60;
  let rel;
  if (days >= 1) {
    rel = hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  } else if (hours >= 1) {
    rel = mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  } else {
    rel = `${Math.max(1, mins)}m`;
  }
  return `${timeStr} (in ${rel})`;
}

/** Utilization 0..1 -> integer percentage string. */
export function formatUtilizationPct(util: number | null | undefined): string {
  if (util == null) return '--%';
  return `${Math.round(util * 100)}%`;
}

/** Utilization 0..1 -> integer percentage clamped to [0, 100] for a bar. */
export function utilizationToBarValue(util: number): number {
  return Math.min(100, Math.max(0, Math.round(util * 100)));
}

/** USD amount formatted as `$x.yz` with fixed 2 decimals. */
export function formatUsd(value: number): string {
  return `$${value.toFixed(2)}`;
}

/** Time since probe (seconds) -> `just now` / `4m ago` / `1h ago`. */
export function formatSinceProbe(lastProbedAtSecs: number | null | undefined): string | null {
  if (lastProbedAtSecs == null) return null;
  const diff = Math.max(0, Math.floor(Date.now() / 1000 - lastProbedAtSecs));
  if (diff < 30) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}
