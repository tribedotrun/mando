import { Terminal as XTerm, type IDisposable } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';
import { ClipboardAddon } from '@xterm/addon-clipboard';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import {
  connectTerminalStream,
  resizeTerminal,
  writeTerminalBytes,
  type TerminalSessionInfo,
} from '#renderer/domains/captain/repo/terminal-api';
import { isRestoredTerminalSession } from '#renderer/domains/captain/terminal/runtime/terminalSession';
import { installTerminalLinkProviders } from '#renderer/domains/captain/terminal/runtime/terminalLinkBridge';
import {
  DEFAULT_ROWS,
  DEFAULT_COLS,
  SCROLLBACK_LINES,
  RESIZE_DEBOUNCE_MS,
  RECONNECT_BASE_MS,
  RECONNECT_MAX_MS,
  RECONNECT_MAX_ATTEMPTS,
  TERMINAL_THEME,
  SEARCH_OPTIONS,
  type TerminalConnectionState,
  type TerminalSearchState,
  emptySearchState,
  writeWithCallback,
  binaryBytes,
  isShiftEnter,
} from '#renderer/domains/captain/terminal/runtime/terminalConfig';
import log from '#renderer/global/service/logger';

interface TerminalRuntimeCallbacks {
  onConnectionStateChange: (state: TerminalConnectionState) => void;
  onSearchStateChange: (state: TerminalSearchState) => void;
  onExit?: (code: number | null) => void;
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
  private exited = false;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
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
    this.disposables.push(...installTerminalLinkProviders(term, this.session.cwd));
    fitAddon.fit();
    await this.syncTerminalSize();
    this.bindTerminalInput();
    this.bindResize(container);
    this.connectStream(true);

    // Re-sync after the browser has laid out the container. Covers races where
    // the initial fit runs before the pane has its final dimensions (e.g. after
    // a daemon restart when the session ID may have changed mid-mount).
    requestAnimationFrame(() => {
      if (!this.disposed && this.fitAddon) {
        this.fitAddon.fit();
        void this.syncTerminalSize();
      }
    });
  }

  dispose(): void {
    this.disposed = true;
    if (this.resizeTimer !== null) clearTimeout(this.resizeTimer);
    this.resizeTimer = null;
    if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
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
    if (!this.term || this.disposed) return;

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
        this.exited = true;
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
          this.reconnectAttempt = 0;
          this.callbacks.onConnectionStateChange('connected');
        }
      },
      (err) => {
        if (this.disposed || this.exited) return;
        log.warn('Terminal stream errored, scheduling reconnect', {
          session: this.session.id,
          attempt: this.reconnectAttempt,
          error: err,
        });
        this.callbacks.onConnectionStateChange('disconnected');
        this.eventSource?.close();
        this.eventSource = null;
        this.scheduleReconnect();
      },
      { replay },
    );
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null || this.disposed || this.exited) return;
    if (this.reconnectAttempt >= RECONNECT_MAX_ATTEMPTS) {
      log.warn('Terminal reconnect attempts exhausted', { session: this.session.id });
      this.term?.write(
        `\r\n\x1b[90m[Connection lost — close this tab and reopen to reconnect]\x1b[0m\r\n`,
      );
      return;
    }
    const delay = Math.min(RECONNECT_BASE_MS * 2 ** this.reconnectAttempt, RECONNECT_MAX_MS);
    this.reconnectAttempt++;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.disposed && !this.exited) {
        this.connectStream(false);
      }
    }, delay);
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
