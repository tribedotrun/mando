import type { ISearchOptions } from '@xterm/addon-search';
import { Terminal as XTerm } from '@xterm/xterm';

export const DEFAULT_ROWS = 32;
export const DEFAULT_COLS = 120;
export const SCROLLBACK_LINES = 10_000;
export const RESIZE_DEBOUNCE_MS = 100;
export const RECONNECT_BASE_MS = 1000;
export const RECONNECT_MAX_MS = 10_000;
export const RECONNECT_MAX_ATTEMPTS = 10;

export const TERMINAL_THEME = {
  background: '#0a0a0a',
  foreground: '#e5e5e5',
  cursor: '#e5e5e5',
  selectionBackground: '#3a3a3a',
  black: '#0a0a0a',
  red: '#ff5555',
  green: '#50fa7b',
  yellow: '#f1fa8c',
  blue: '#6272a4',
  magenta: '#ff79c6',
  cyan: '#8be9fd',
  white: '#e0e0e0',
};

export const SEARCH_OPTIONS: ISearchOptions = {
  incremental: true,
  decorations: {
    matchBackground: '#2a3348',
    matchBorder: '#5d84ff',
    matchOverviewRuler: '#5d84ff',
    activeMatchBackground: '#4b5d89',
    activeMatchBorder: '#91a8ff',
    activeMatchColorOverviewRuler: '#91a8ff',
  },
};

export type TerminalConnectionState = 'connecting' | 'connected' | 'disconnected';

export interface TerminalSearchState {
  open: boolean;
  query: string;
  resultCount: number;
  resultIndex: number;
}

export function emptySearchState(): TerminalSearchState {
  return { open: false, query: '', resultCount: 0, resultIndex: -1 };
}

export function writeWithCallback(term: XTerm, data: string | Uint8Array): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

export function binaryBytes(data: string): Uint8Array {
  const bytes = new Uint8Array(data.length);
  for (let i = 0; i < data.length; i++) bytes[i] = data.charCodeAt(i);
  return bytes;
}

export function isShiftEnter(event: KeyboardEvent): boolean {
  return (
    event.key === 'Enter' && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey
  );
}
