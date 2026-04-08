import { useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { SearchAddon } from '@xterm/addon-search';
import { writeTerminalBytes, resizeTerminal, connectTerminalStream } from '#renderer/api-terminal';
import log from '#renderer/logger';

interface UseTerminalOptions {
  sessionId: string;
  onExit?: (code: number | null) => void;
}

interface UseTerminalResult {
  containerRef: React.RefObject<HTMLDivElement | null>;
  terminal: Terminal | null;
  isConnected: boolean;
}

export function useTerminal({ sessionId, onExit }: UseTerminalOptions): UseTerminalResult {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const esRef = useRef<EventSource | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  useMountEffect(() => {
    if (!containerRef.current || !sessionId) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'SF Mono, Menlo, monospace',
      theme: {
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
      },
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    const searchAddon = new SearchAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(searchAddon);

    term.open(containerRef.current);

    // Try WebGL renderer, fall back to canvas.
    try {
      const webglAddon = new WebglAddon();
      webglAddon.onContextLoss(() => webglAddon.dispose());
      term.loadAddon(webglAddon);
    } catch {
      log.warn('WebGL addon failed, using canvas renderer');
    }

    fitAddon.fit();
    termRef.current = term;
    fitAddonRef.current = fitAddon;

    // Send measured size to backend immediately so the PTY matches the
    // actual container before Claude Code starts rendering its status bar.
    const { rows, cols } = term;
    void resizeTerminal(sessionId, rows, cols).catch((err) =>
      log.warn('Initial resize failed', err),
    );

    // Send user input to daemon.
    const inputDisposable = term.onData((data) => {
      const encoder = new TextEncoder();
      void writeTerminalBytes(sessionId, encoder.encode(data)).catch((err) =>
        log.warn('Terminal write failed', err),
      );
    });

    // Binary input (for things like Ctrl+C raw bytes).
    const binaryDisposable = term.onBinary((data) => {
      const bytes = new Uint8Array(data.length);
      for (let i = 0; i < data.length; i++) {
        bytes[i] = data.charCodeAt(i);
      }
      void writeTerminalBytes(sessionId, bytes).catch((err) =>
        log.warn('Terminal binary write failed', err),
      );
    });

    // Connect SSE stream for output.
    const es = connectTerminalStream(
      sessionId,
      (data) => term.write(data),
      (code) => {
        setIsConnected(false);
        term.write(`\r\n\x1b[90m[Process exited with code ${code ?? 'unknown'}]\x1b[0m\r\n`);
        onExit?.(code);
      },
      () => {
        setIsConnected(false);
      },
    );
    esRef.current = es;
    setIsConnected(true);

    // Handle resize.
    const resizeObs = new ResizeObserver(() => {
      fitAddon.fit();
      const { rows, cols } = term;
      void resizeTerminal(sessionId, rows, cols).catch((err) => log.warn('Resize failed', err));
    });
    resizeObs.observe(containerRef.current);

    return () => {
      resizeObs.disconnect();
      inputDisposable.dispose();
      binaryDisposable.dispose();
      es.close();
      esRef.current = null;
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
      setIsConnected(false);
    };
  });

  return { containerRef, terminal: termRef.current, isConnected };
}
