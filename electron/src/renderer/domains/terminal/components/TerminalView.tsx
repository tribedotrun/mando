import React from 'react';
import '@xterm/xterm/css/xterm.css';
import { useTerminal } from '#renderer/domains/terminal/hooks/useTerminal';

interface TerminalViewProps {
  sessionId: string;
  onExit?: (code: number | null) => void;
}

export function TerminalView({ sessionId, onExit }: TerminalViewProps) {
  const { containerRef, isConnected } = useTerminal({ sessionId, onExit });

  return (
    <div
      className="relative h-full w-full overflow-hidden"
      style={{ backgroundColor: 'var(--color-bg)' }}
    >
      {!isConnected && (
        <div className="absolute top-1 right-2 z-10 text-caption text-text-3">Disconnected</div>
      )}
      <div ref={containerRef} className="h-full w-full p-2" />
    </div>
  );
}
