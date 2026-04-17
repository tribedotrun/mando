import { X, Plus, Circle } from 'lucide-react';
import {
  isRestoredTerminalSession,
  TERMINAL_AGENTS,
} from '#renderer/domains/captain/terminal/runtime/terminalSession';
import type { TerminalSessionInfo } from '#renderer/domains/captain/terminal/types';

interface TerminalTabBarProps {
  sessions: TerminalSessionInfo[];
  activeTab: string | null;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onNewTerminal: (agent: 'claude' | 'codex') => void;
}

export function TerminalTabBar({
  sessions,
  activeTab,
  onSelectTab,
  onCloseTab,
  onNewTerminal,
}: TerminalTabBarProps) {
  return (
    <div className="flex shrink-0 items-center px-4" style={{ height: 36 }}>
      {sessions.map((session) => (
        <div
          key={session.id}
          onClick={() => onSelectTab(session.id)}
          className="flex cursor-pointer items-center gap-1.5 px-3"
          style={{
            height: '100%',
            borderBottom:
              activeTab === session.id ? '2px solid var(--color-accent)' : '2px solid transparent',
            color: activeTab === session.id ? 'var(--color-text-1)' : 'var(--color-text-3)',
            fontSize: 13,
          }}
        >
          <Circle
            size={6}
            fill={
              session.running
                ? 'var(--color-success)'
                : isRestoredTerminalSession(session)
                  ? 'var(--color-accent)'
                  : 'var(--color-text-4)'
            }
            stroke="none"
          />
          <span
            className="max-w-[160px] truncate"
            title={session.name || `${session.agent} ${session.id.slice(0, 6)}`}
          >
            {session.name || `${session.agent} ${session.id.slice(0, 6)}`}
          </span>
          <X
            size={12}
            className="opacity-40 hover:opacity-100"
            style={{ cursor: 'pointer' }}
            onClick={(event) => {
              event.stopPropagation();
              onCloseTab(session.id);
            }}
          />
        </div>
      ))}

      {sessions.length > 0 && <div className="mx-2 h-4 border-l border-border-subtle" />}

      {TERMINAL_AGENTS.map((agent) => (
        <button
          key={agent.id}
          onClick={() => onNewTerminal(agent.id)}
          className="flex cursor-pointer items-center gap-1.5 rounded px-2.5 py-1 text-caption text-text-3 transition-colors hover:bg-surface-2 hover:text-text-2"
          style={{ background: 'none', border: 'none' }}
        >
          <Plus size={10} />
          {agent.label}
        </button>
      ))}
    </div>
  );
}
