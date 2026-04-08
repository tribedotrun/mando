import { useCallback, useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { useTerminalStore } from '#renderer/domains/terminal/stores/terminalStore';
import { X, Plus, Terminal, Circle } from 'lucide-react';
import { toast } from 'sonner';
import log from '#renderer/logger';

interface TerminalTabProps {
  /** Project name for creating new terminals. */
  project: string;
  /** Working directory (worktree path) for terminals. */
  cwd: string;
  /** If set, auto-resume this CC session on mount. */
  resumeSessionId?: string | null;
  /** Called after resumeSessionId is consumed to prevent duplicate resumes. */
  onResumeConsumed?: () => void;
}

export function TerminalTab({ project, cwd, resumeSessionId, onResumeConsumed }: TerminalTabProps) {
  const {
    sessions,
    addSession,
    removeSession,
    updateSession,
    fetch: fetchSessions,
  } = useTerminalStore();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const resumedRef = useRef(false);

  // Hydrate existing sessions from daemon on mount, auto-select last session.
  useMountEffect(() => {
    void fetchSessions().then(() => {
      const store = useTerminalStore.getState();
      const relevant = store.sessions.filter((s) => s.project === project && s.cwd === cwd);
      if (relevant.length > 0 && !activeTab) {
        setActiveTab(relevant[relevant.length - 1].id);
      }
    });
  });

  // Auto-resume a session if resumeSessionId is provided.
  useMountEffect(() => {
    if (!resumeSessionId || resumedRef.current) return;
    resumedRef.current = true;
    void addSession({
      project,
      cwd,
      agent: 'claude',
      resume_session_id: resumeSessionId,
    })
      .then((session) => {
        setActiveTab(session.id);
        onResumeConsumed?.();
      })
      .catch((e) => log.error('Failed to resume terminal session', e));
  });

  // Filter sessions for this project/cwd.
  const relevantSessions = sessions.filter((s) => s.project === project && s.cwd === cwd);

  const handleNewTerminal = useCallback(
    async (agent: 'claude' | 'codex') => {
      try {
        const session = await addSession({ project, cwd, agent });
        setActiveTab(session.id);
      } catch (e) {
        log.error('Failed to create terminal', e);
        toast.error(e instanceof Error ? e.message : 'Failed to create terminal');
      }
    },
    [project, cwd, addSession],
  );

  const handleCloseTab = useCallback(
    (id: string) => {
      void removeSession(id)
        .then(() => {
          if (activeTab === id) {
            const remaining = relevantSessions.filter((s) => s.id !== id);
            setActiveTab(remaining.length > 0 ? remaining[0].id : null);
          }
        })
        .catch((err) => console.error('Failed to close tab', err));
    },
    [activeTab, relevantSessions, removeSession],
  );

  const handleExit = useCallback(
    (id: string, code: number | null) => {
      updateSession(id, { running: false, exit_code: code });
    },
    [updateSession],
  );

  // Empty state: no active terminals.
  if (relevantSessions.length === 0 && !activeTab) {
    return (
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          gap: 16,
          color: 'var(--text-3)',
        }}
      >
        <Terminal size={32} strokeWidth={1.5} />
        <p style={{ fontSize: 13 }}>No active terminals</p>
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            className="btn btn-primary"
            onClick={() => void handleNewTerminal('claude')}
            style={{ fontSize: 13 }}
          >
            + Claude terminal
          </button>
          <button
            className="btn btn-secondary"
            onClick={() => void handleNewTerminal('codex')}
            style={{ fontSize: 13 }}
          >
            + Codex terminal
          </button>
        </div>
        <p style={{ fontSize: 11, color: 'var(--text-4)' }}>Opens in {cwd}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Sub-tabs for open terminal sessions */}
      <div className="flex shrink-0 items-center pl-2 text-caption" style={{ height: 32 }}>
        {relevantSessions.map((s) => (
          <div
            key={s.id}
            onClick={() => setActiveTab(s.id)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '0 10px',
              height: '100%',
              cursor: 'pointer',
              borderBottom:
                activeTab === s.id ? '2px solid var(--accent)' : '2px solid transparent',
              color: activeTab === s.id ? 'var(--text-1)' : 'var(--text-3)',
            }}
          >
            <Circle size={6} fill={s.running ? 'var(--green)' : 'var(--text-4)'} stroke="none" />
            <span>
              {s.agent} {s.id.slice(0, 6)}
            </span>
            <X
              size={12}
              style={{ opacity: 0.5, cursor: 'pointer' }}
              onClick={(e) => {
                e.stopPropagation();
                void handleCloseTab(s.id);
              }}
            />
          </div>
        ))}
        <button
          onClick={() => void handleNewTerminal('claude')}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            padding: '0 8px',
            color: 'var(--text-3)',
            height: '100%',
            display: 'flex',
            alignItems: 'center',
          }}
          title="New terminal"
        >
          <Plus size={14} />
        </button>
      </div>

      {/* Active terminal */}
      <div className="min-h-0 flex-1">
        {activeTab && (
          <TerminalView
            key={activeTab}
            sessionId={activeTab}
            onExit={(code) => handleExit(activeTab, code)}
          />
        )}
      </div>
    </div>
  );
}
