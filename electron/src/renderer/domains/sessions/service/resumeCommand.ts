type ResumeCommandProvider = 'claude' | 'codex' | 'opencode';

/** Builds a provider-native terminal resume command when one is available. */
export function buildResumeCmd(
  sessionId: string,
  provider: ResumeCommandProvider | undefined,
  cwd?: string | null,
): string | null {
  if (!provider) return null;
  if (provider === 'codex') {
    return cwd ? `cd "${cwd}" && codex resume ${sessionId}` : `codex resume ${sessionId}`;
  }
  if (provider === 'opencode') {
    return cwd
      ? `cd "${cwd}" && opencode --session ${sessionId}`
      : `opencode --session ${sessionId}`;
  }
  return cwd ? `cd "${cwd}" && claude -r ${sessionId}` : `claude -r ${sessionId}`;
}
