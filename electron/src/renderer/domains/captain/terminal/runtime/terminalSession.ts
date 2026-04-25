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
