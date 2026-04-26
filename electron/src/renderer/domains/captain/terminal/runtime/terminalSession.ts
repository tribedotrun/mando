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
 * A workbench has a primary cwd (its worktree) and may have additional
 * acceptable cwds — notably the project root for clarifier-resumed
 * sessions, whose stored cwd is the project root rather than the worktree
 * (clarifier runs from the project root, see captain::runtime::clarifier
 * `resolve_clarifier_cwd`). The cc_sessions row's cwd is preserved when
 * resuming, so the resumed terminal also lives at the project root.
 *
 * Filtering by project alone would leak sessions from sibling workbenches;
 * filtering by worktree alone drops the resumed clarifier. The accepted
 * cwd set is the workbench's "address book": one or more concrete paths
 * that map back to this workbench.
 */
export function selectWorkbenchTerminalSessions<T extends { project: string; cwd: string }>(
  sessions: readonly T[],
  project: string,
  acceptedCwds: readonly string[],
): T[] {
  return sessions.filter((s) => s.project === project && acceptedCwds.includes(s.cwd));
}
