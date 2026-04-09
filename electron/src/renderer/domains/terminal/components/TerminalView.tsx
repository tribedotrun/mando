import React from 'react';
import { Search } from 'lucide-react';
import '@xterm/xterm/css/xterm.css';
import { TerminalSearchBar } from '#renderer/domains/terminal/components/TerminalSearchBar';
import { useTerminalRuntime } from '#renderer/domains/terminal/hooks/useTerminalRuntime';
import { isRestoredTerminalSession } from '#renderer/domains/terminal/runtime/terminalSession';
import type { TerminalSessionInfo } from '#renderer/domains/terminal/types';

interface TerminalViewProps {
  session: TerminalSessionInfo;
  onExit?: (code: number | null) => void;
}

export function TerminalView({ session, onExit }: TerminalViewProps) {
  const {
    containerRef,
    connectionState,
    search,
    openSearch,
    closeSearch,
    setSearchQuery,
    findNext,
    findPrevious,
  } = useTerminalRuntime({ session, onExit });
  const restored = isRestoredTerminalSession(session);
  const connectionLabel = connectionState === 'connecting' ? 'Connecting\u2026' : 'Disconnected';

  const handleRootClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (target instanceof Element) {
      if (target.closest('[data-terminal-overlay="true"]')) return;
      if (target.closest('button, input, textarea, select, a, [role="button"]')) return;
    }
    containerRef.current?.querySelector('textarea')?.focus();
  };

  return (
    <div
      className="relative h-full w-full overflow-hidden"
      style={{ backgroundColor: 'var(--color-bg)' }}
      onClick={handleRootClick}
    >
      {search.open && (
        <div data-terminal-overlay="true">
          <TerminalSearchBar
            query={search.query}
            resultCount={search.resultCount}
            resultIndex={search.resultIndex}
            onChange={setSearchQuery}
            onNext={findNext}
            onPrevious={findPrevious}
            onClose={closeSearch}
          />
        </div>
      )}
      <div
        data-terminal-overlay="true"
        className="absolute top-1 right-2 z-10 flex items-center gap-2"
      >
        {!restored && connectionState !== 'connected' && (
          <div className="text-caption text-text-3">{connectionLabel}</div>
        )}
        <button
          type="button"
          className="flex items-center gap-1 rounded px-2 py-1 text-caption text-text-3 hover:bg-surface-2 hover:text-text-2"
          onClick={openSearch}
          aria-label="Search terminal"
        >
          <Search size={14} />
          Search
        </button>
      </div>
      <div ref={containerRef} className="h-full w-full p-2" />
    </div>
  );
}
