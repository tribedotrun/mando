export interface ClaudeCheckResult {
  installed: boolean;
  version: string | null;
  works: boolean;
  checkFailed?: boolean;
  error?: string;
}

interface SetupResult {
  ok: boolean;
  daemonNotified?: boolean;
  launchdInstalled?: boolean;
  error?: string;
}

/** Formats a partial-failure setup result into a user-visible error message. */
export function formatSetupError(result: SetupResult): string {
  const parts: string[] = [];
  if (!result.daemonNotified) parts.push('daemon did not respond');
  if (!result.launchdInstalled) parts.push('background service install failed');
  const detail = parts.join(', ');
  const suffix = result.error ? `: ${result.error}` : '';
  return `Setup partially failed (${detail})${suffix}. You can continue, but restart Mando if things feel stuck.`;
}
