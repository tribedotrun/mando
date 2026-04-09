import { useRef, useState } from 'react';
import type { TerminalSessionInfo } from '#renderer/api-terminal';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import {
  TerminalRuntime,
  type TerminalConnectionState,
  type TerminalSearchState,
} from '#renderer/domains/terminal/runtime/terminalRuntime';
import log from '#renderer/logger';

interface UseTerminalRuntimeOptions {
  session: TerminalSessionInfo;
  onExit?: (code: number | null) => void;
}

interface UseTerminalRuntimeResult {
  containerRef: React.RefObject<HTMLDivElement | null>;
  connectionState: TerminalConnectionState;
  search: TerminalSearchState;
  openSearch: () => void;
  closeSearch: () => void;
  setSearchQuery: (query: string) => void;
  findNext: () => void;
  findPrevious: () => void;
}

const EMPTY_SEARCH: TerminalSearchState = {
  open: false,
  query: '',
  resultCount: 0,
  resultIndex: -1,
};

export function useTerminalRuntime({
  session,
  onExit,
}: UseTerminalRuntimeOptions): UseTerminalRuntimeResult {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const runtimeRef = useRef<TerminalRuntime | null>(null);
  const [connectionState, setConnectionState] = useState<TerminalConnectionState>('connecting');
  const [search, setSearch] = useState<TerminalSearchState>(EMPTY_SEARCH);

  runtimeRef.current?.updateSession(session, onExit);

  useMountEffect(() => {
    if (!containerRef.current) return;

    const runtime = new TerminalRuntime(session, {
      onConnectionStateChange: setConnectionState,
      onSearchStateChange: setSearch,
      onExit,
    });
    runtimeRef.current = runtime;

    void runtime.attach(containerRef.current).catch((err) => {
      log.error('Failed to attach terminal runtime', err);
      setConnectionState('disconnected');
    });

    return () => {
      runtime.dispose();
      runtimeRef.current = null;
    };
  });

  useViewKeyHandler((key, event) => runtimeRef.current?.handleViewKey(key, event), true);

  return {
    containerRef,
    connectionState,
    search,
    openSearch: () => runtimeRef.current?.openSearch(),
    closeSearch: () => runtimeRef.current?.closeSearch(),
    setSearchQuery: (query) => runtimeRef.current?.setSearchQuery(query),
    findNext: () => runtimeRef.current?.findNext(),
    findPrevious: () => runtimeRef.current?.findPrevious(),
  };
}
