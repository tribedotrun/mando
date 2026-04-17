import type { TerminalSessionInfo } from '#renderer/domains/captain/repo/terminal-api';

/** Available terminal agent types. */
export const TERMINAL_AGENTS = [
  { id: 'claude' as const, label: 'claude', icon: '*' },
  { id: 'codex' as const, label: 'codex', icon: '@' },
] as const;

export type TerminalSessionState = 'live' | 'restored' | 'exited';

export function getTerminalSessionState(session: TerminalSessionInfo): TerminalSessionState {
  if (session.state) return session.state;
  if (session.restored) return 'restored';
  return session.running ? 'live' : 'exited';
}

export function isRestoredTerminalSession(session: TerminalSessionInfo): boolean {
  return getTerminalSessionState(session) === 'restored';
}

export function isLiveTerminalSession(session: TerminalSessionInfo): boolean {
  return getTerminalSessionState(session) === 'live';
}
