import { Terminal as XTerm, type IDisposable } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon, type ISearchOptions } from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';
import { ClipboardAddon } from '@xterm/addon-clipboard';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import {
  connectTerminalStream,
  resizeTerminal,
  writeTerminalBytes,
  type TerminalSessionInfo,
} from '#renderer/api-terminal';
import {
  createFileLinkProvider,
  createUrlLinkProvider,
} from '#renderer/domains/terminal/runtime/terminalLinks';
import { isRestoredTerminalSession } from '#renderer/domains/terminal/runtime/terminalSession';
import log from '#renderer/logger';

const DEFAULT_ROWS = 32;
const DEFAULT_COLS = 120;
const SCROLLBACK_LINES = 10_000;
const RESIZE_DEBOUNCE_MS = 100;

const TERMINAL_THEME = {
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

const SEARCH_OPTIONS: ISearchOptions = {
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

interface TerminalRuntimeCallbacks {
  onConnectionStateChange: (state: TerminalConnectionState) => void;
  onSearchStateChange: (state: TerminalSearchState) => void;
  onExit?: (code: number | null) => void;
}

function emptySearchState(): TerminalSearchState {
  return { open: false, query: '', resultCount: 0, resultIndex: -1 };
}

function writeWithCallback(term: XTerm, data: string | Uint8Array): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

function binaryBytes(data: string): Uint8Array {
  const bytes = new Uint8Array(data.length);
  for (let i = 0; i < data.length; i++) bytes[i] = data.charCodeAt(i);
  return bytes;
}

function isShiftEnter(event: KeyboardEvent): boolean {
  return (
    event.key === 'Enter' && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey
  );
}

export class TerminalRuntime {
  private readonly callbacks: TerminalRuntimeCallbacks;
  private session: TerminalSessionInfo;
  private term: XTerm | null = null;
  private fitAddon: FitAddon | null = null;
  private searchAddon: SearchAddon | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private resizeTimer: ReturnType<typeof setTimeout> | null = null;
  private eventSource: EventSource | null = null;
  private disposables: IDisposable[] = [];
  private disposed = false;
  private searchState = emptySearchState();

  constructor(session: TerminalSessionInfo, callbacks: TerminalRuntimeCallbacks) {
    this.session = session;
    this.callbacks = callbacks;
  }

  updateSession(session: TerminalSessionInfo, onExit?: (code: number | null) => void): void {
    this.session = session;
    this.callbacks.onExit = onExit;
  }

  async attach(container: HTMLDivElement): Promise<void> {
    const term = new XTerm({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'SF Mono, Menlo, monospace',
      theme: TERMINAL_THEME,
      allowProposedApi: true,
      rows: DEFAULT_ROWS,
      cols: DEFAULT_COLS,
      scrollback: SCROLLBACK_LINES,
      smoothScrollDuration: 80,
    });
    const fitAddon = new FitAddon();
    const searchAddon = new SearchAddon({ highlightLimit: 500 });
    const clipboardAddon = new ClipboardAddon();
    const unicodeAddon = new Unicode11Addon();

    term.loadAddon(fitAddon);
    term.loadAddon(searchAddon);
    term.loadAddon(clipboardAddon);
    term.loadAddon(unicodeAddon);
    term.unicode.activeVersion = '11';

    this.term = term;
    this.fitAddon = fitAddon;
    this.searchAddon = searchAddon;

    this.disposables.push(
      searchAddon.onDidChangeResults(({ resultCount, resultIndex }) => {
        this.setSearchState({
          ...this.searchState,
          resultCount,
          resultIndex,
        });
      }),
    );

    if (this.disposed) return;

    term.open(container);
    this.installWebglAddon(term);
    this.installLinkProviders(term);
    fitAddon.fit();
    await this.syncTerminalSize();
    this.bindTerminalInput();
    this.bindResize(container);
    this.connectStream(true);
  }

  dispose(): void {
    this.disposed = true;
    if (this.resizeTimer !== null) clearTimeout(this.resizeTimer);
    this.resizeTimer = null;
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    this.eventSource?.close();
    this.eventSource = null;
    for (const disposable of this.disposables.splice(0)) disposable.dispose();
    this.term?.dispose();
    this.term = null;
    this.fitAddon = null;
    this.searchAddon = null;
    this.callbacks.onConnectionStateChange('disconnected');
  }

  handleViewKey(key: string, event: KeyboardEvent): void {
    if (key === 'Mod+f') {
      event.preventDefault();
      this.openSearch();
      return;
    }

    if (key === 'Escape' && this.searchState.open) {
      event.preventDefault();
      this.closeSearch();
    }
  }

  openSearch(): void {
    this.setSearchState({ ...this.searchState, open: true });
  }

  closeSearch(): void {
    this.setSearchState({ ...this.searchState, open: false });
  }

  setSearchQuery(query: string): void {
    this.setSearchState({ ...this.searchState, open: true, query });
    if (!query) {
      this.term?.clearSelection();
      this.setSearchState({
        ...this.searchState,
        open: true,
        query,
        resultCount: 0,
        resultIndex: -1,
      });
      return;
    }
    this.searchAddon?.findNext(query, SEARCH_OPTIONS);
  }

  findNext(): void {
    if (!this.searchState.query) return;
    this.searchAddon?.findNext(this.searchState.query, SEARCH_OPTIONS);
  }

  findPrevious(): void {
    if (!this.searchState.query) return;
    this.searchAddon?.findPrevious(this.searchState.query, SEARCH_OPTIONS);
  }

  private installWebglAddon(term: XTerm): void {
    try {
      const webglAddon = new WebglAddon();
      webglAddon.onContextLoss(() => webglAddon.dispose());
      term.loadAddon(webglAddon);
    } catch (err) {
      log.warn('WebGL addon failed, using DOM renderer', err);
    }
  }

  private installLinkProviders(term: XTerm): void {
    const openUrl = (url: string) => window.mandoAPI.openExternalUrl(url);
    const resolvePath = (input: string, cwd: string) =>
      window.mandoAPI.resolveLocalPath(input, cwd);
    const openPath = (filePath: string) => window.mandoAPI.openLocalPath(filePath);

    this.disposables.push(
      term.registerLinkProvider(createUrlLinkProvider({ terminal: term, openUrl })),
    );
    this.disposables.push(
      term.registerLinkProvider(
        createFileLinkProvider({
          terminal: term,
          cwd: this.session.cwd,
          resolvePath,
          openPath,
        }),
      ),
    );
  }

  private bindTerminalInput(): void {
    if (!this.term) return;

    this.term.attachCustomKeyEventHandler((event) => {
      if (this.session.agent !== 'claude' || !isShiftEnter(event)) return true;

      // Block both keydown AND keypress to prevent xterm from also sending
      // a bare \r on the keypress event. Only send the sequence on keydown.
      if (event.type === 'keydown') {
        const encoder = new TextEncoder();
        void writeTerminalBytes(this.session.id, encoder.encode('\x1b\r')).catch((err) =>
          log.warn('Terminal soft-enter write failed', err),
        );
      }
      return false;
    });

    this.disposables.push(
      this.term.onData((data) => {
        const encoder = new TextEncoder();
        void writeTerminalBytes(this.session.id, encoder.encode(data)).catch((err) =>
          log.warn('Terminal write failed', err),
        );
      }),
    );

    this.disposables.push(
      this.term.onBinary((data) => {
        void writeTerminalBytes(this.session.id, binaryBytes(data)).catch((err) =>
          log.warn('Terminal binary write failed', err),
        );
      }),
    );
  }

  private bindResize(container: HTMLDivElement): void {
    this.resizeObserver = new ResizeObserver(() => {
      if (this.resizeTimer !== null) clearTimeout(this.resizeTimer);
      this.resizeTimer = setTimeout(() => {
        this.resizeTimer = null;
        this.fitAddon?.fit();
        void this.syncTerminalSize();
      }, RESIZE_DEBOUNCE_MS);
    });
    this.resizeObserver.observe(container);
  }

  private connectStream(replay: boolean): void {
    if (!this.term) return;

    this.callbacks.onConnectionStateChange('connecting');
    this.eventSource?.close();
    this.eventSource = connectTerminalStream(
      this.session.id,
      (data) => {
        const term = this.term;
        if (!term || this.disposed) return;
        void writeWithCallback(term, data).catch((err) =>
          log.warn('Terminal output write failed', err),
        );
      },
      (code) => {
        this.callbacks.onConnectionStateChange('disconnected');
        if (!isRestoredTerminalSession(this.session)) {
          this.term?.write(
            `\r\n\x1b[90m[Process exited with code ${code ?? 'unknown'}]\x1b[0m\r\n`,
          );
        }
        this.callbacks.onExit?.(code);
      },
      () => {
        if (!this.disposed) {
          this.callbacks.onConnectionStateChange('connected');
        }
      },
      (err) => {
        log.warn('Terminal stream errored', err);
        this.callbacks.onConnectionStateChange('disconnected');
      },
      { replay },
    );
  }

  private async syncTerminalSize(): Promise<void> {
    if (!this.term) return;
    const { rows, cols } = this.term;
    await resizeTerminal(this.session.id, rows, cols).catch((err) =>
      log.warn('Terminal resize failed', err),
    );
  }

  private setSearchState(next: TerminalSearchState): void {
    this.searchState = next;
    this.callbacks.onSearchStateChange(next);
  }
}
