import type {
  TerminalSessionInfo,
  TerminalState,
} from '#renderer/domains/captain/repo/terminal-api';

/** Available terminal agent types. */
export const TERMINAL_AGENTS = [
  { id: 'claude' as const, label: 'claude', icon: '*' },
  { id: 'codex' as const, label: 'codex', icon: '@' },
] as const;

export function getTerminalSessionState(session: TerminalSessionInfo): TerminalState {
  if (session.state) return session.state;
  if (session.restored) return 'restored';
  return session.running ? 'live' : 'exited';
}

export function isRestoredTerminalSession(session: TerminalSessionInfo): boolean {
  return getTerminalSessionState(session) === 'restored';
}

/**
 * Pick the terminal sessions that belong to a given workbench.
 *
 * Sessions are stamped with their owning workbench id at create time
 * (see `routes_terminal::post_terminal_create`), so the renderer scopes
 * by identity instead of cwd. The previous cwd-based filter widened
 * `acceptedCwds` to include the project root for clarifier-resumed
 * sessions, which leaked any session whose cwd happened to equal the
 * project root into every other workbench in the same project.
 */
export function selectWorkbenchTerminalSessions<T extends { workbenchId: number }>(
  sessions: readonly T[],
  workbenchId: number,
): T[] {
  return sessions.filter((s) => s.workbenchId === workbenchId);
}
