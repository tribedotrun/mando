import type { TerminalSessionInfo } from '#renderer/api-terminal';

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
