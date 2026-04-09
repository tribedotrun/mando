import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { isRestoredTerminalSession } from '#renderer/domains/terminal/runtime/terminalSession';
import { useTerminalList, type TerminalSessionInfo } from '#renderer/hooks/queries';
import { useTerminalCreate, useTerminalDelete } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
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
  const { data: sessions = [] } = useTerminalList();
  const createMutation = useTerminalCreate();
  const deleteMutation = useTerminalDelete();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [exitStates, setExitStates] = useState<
    Record<string, { running: boolean; exit_code: number | null }>
  >({});
  const resumedRef = useRef(false);
  const initialSelectedRef = useRef(false);

  // Auto-select last relevant session once data arrives (derived during render).
  if (!initialSelectedRef.current && !activeTab && sessions.length > 0) {
    const relevant = sessions.filter((s) => s.project === project && s.cwd === cwd);
    if (relevant.length > 0) {
      initialSelectedRef.current = true;
      setActiveTab(relevant[relevant.length - 1].id);
    }
  }

  // Auto-resume a session if resumeSessionId is provided.
  useMountEffect(() => {
    if (!resumeSessionId || resumedRef.current) return;
    resumedRef.current = true;
    createMutation.mutate(
      { project, cwd, agent: 'claude', resume_session_id: resumeSessionId },
      {
        onSuccess: (session) => {
          setActiveTab(session.id);
          onResumeConsumed?.();
        },
        onError: (e) => log.error('Failed to resume terminal session', e),
      },
    );
  });

  // Merge exit states into sessions for rendering.
  const sessionsWithExitState = sessions.map((s) => {
    const override = exitStates[s.id];
    return override ? { ...s, ...override } : s;
  });

  // Filter sessions for this project/cwd.
  const relevantSessions = sessionsWithExitState.filter(
    (s) => s.project === project && s.cwd === cwd,
  );

  const handleNewTerminal = useCallback(
    async (agent: 'claude' | 'codex') => {
      try {
        const session = await createMutation.mutateAsync({ project, cwd, agent });
        setActiveTab(session.id);
      } catch (e) {
        log.error('Failed to create terminal', e);
        toast.error(e instanceof Error ? e.message : 'Failed to create terminal');
      }
    },
    [project, cwd, createMutation],
  );

  const handleCloseTab = useCallback(
    (id: string) => {
      deleteMutation.mutate(
        { id },
        {
          onSuccess: () => {
            if (activeTab === id) {
              const remaining = relevantSessions.filter((s) => s.id !== id);
              setActiveTab(remaining.length > 0 ? remaining[0].id : null);
            }
          },
          onError: (err) => console.error('Failed to close tab', err),
        },
      );
    },
    [activeTab, relevantSessions, deleteMutation],
  );

  const handleExit = useCallback(
    (id: string, code: number | null) => {
      setExitStates((prev) => ({ ...prev, [id]: { running: false, exit_code: code } }));
      // Also update the React Query cache so other consumers see the change
      queryClient.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old?.map((s) => (s.id === id ? { ...s, running: false, exit_code: code } : s)),
      );
    },
    [queryClient],
  );
  const handleStartShell = useCallback(
    async (sessionId: string) => {
      const session = relevantSessions.find((s) => s.id === sessionId);
      if (!session) return;
      try {
        const next = await createMutation.mutateAsync({
          project: session.project,
          cwd: session.cwd,
          agent: session.agent,
          ...(session.agent === 'claude' ? { resume_session_id: '' } : {}),
        });
        setActiveTab(next.id);
        await deleteMutation.mutateAsync({ id: sessionId });
      } catch (err) {
        log.error('Failed to replace restored terminal', err);
        toast.error(err instanceof Error ? err.message : 'Failed to start terminal');
      }
    },
    [createMutation, deleteMutation, relevantSessions],
  );

  const activeSession = activeTab
    ? (relevantSessions.find((session) => session.id === activeTab) ?? null)
    : null;

  const autoResumeRef = useRef<string | null>(null);
  if (
    activeSession &&
    isRestoredTerminalSession(activeSession) &&
    autoResumeRef.current !== activeSession.id
  ) {
    autoResumeRef.current = activeSession.id;
    queueMicrotask(() => void handleStartShell(activeSession.id));
  }

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
            <Circle
              size={6}
              fill={
                s.running
                  ? 'var(--green)'
                  : isRestoredTerminalSession(s)
                    ? 'var(--accent)'
                    : 'var(--text-4)'
              }
              stroke="none"
            />
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
        {activeSession && (
          <TerminalView
            key={activeSession.id}
            session={activeSession}
            onExit={(code) => handleExit(activeSession.id, code)}
          />
        )}
      </div>
    </div>
  );
}
