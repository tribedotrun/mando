import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { isRestoredTerminalSession } from '#renderer/domains/terminal/runtime/terminalSession';
import { useTerminalList, type TerminalSessionInfo } from '#renderer/hooks/queries';
import { useTerminalCreate, useTerminalDelete } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import { X, Plus, Circle, ArrowRight } from 'lucide-react';

interface StandaloneTerminalProps {
  project: string;
  cwd: string;
  label: string;
  onAdopt?: (cwd: string) => void;
  onClose?: () => void;
}

export function StandaloneTerminal({
  project,
  cwd,
  label,
  onAdopt,
  onClose,
}: StandaloneTerminalProps) {
  const { data: sessions = [] } = useTerminalList();
  const createMutation = useTerminalCreate();
  const deleteMutation = useTerminalDelete();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [adopting, setAdopting] = useState(false);
  const [startingShellForId, setStartingShellForId] = useState<string | null>(null);
  const [exitStates, setExitStates] = useState<
    Record<string, { running: boolean; exit_code: number | null }>
  >({});

  const sessionsWithExitState = sessions.map((session) => {
    const override = exitStates[session.id];
    return override ? { ...session, ...override } : session;
  });

  const relevantSessions = sessionsWithExitState.filter(
    (session) => session.project === project && session.cwd === cwd,
  );

  const handleNewTerminal = useCallback(
    async (agent: 'claude' | 'codex') => {
      const session = await createMutation.mutateAsync({ project, cwd, agent });
      setActiveTab(session.id);
    },
    [createMutation, cwd, project],
  );

  const handleCloseTab = useCallback(
    (id: string) => {
      deleteMutation.mutate(
        { id },
        {
          onSuccess: () => {
            if (activeTab === id) {
              const remaining = relevantSessions.filter((session) => session.id !== id);
              setActiveTab(remaining.length > 0 ? remaining[0].id : null);
            }
          },
          onError: (err) => console.error('Failed to close tab', err),
        },
      );
    },
    [activeTab, deleteMutation, relevantSessions],
  );

  const handleAdopt = useCallback(() => {
    setAdopting(true);
    onAdopt?.(cwd);
  }, [cwd, onAdopt]);

  const handleStartShell = useCallback(
    async (sessionId: string) => {
      const session = relevantSessions.find((candidate) => candidate.id === sessionId);
      if (!session) return;

      setStartingShellForId(sessionId);
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
        console.error('Failed to replace restored terminal', err);
      } finally {
        setStartingShellForId((current) => (current === sessionId ? null : current));
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
    autoResumeRef.current !== activeSession.id &&
    !startingShellForId
  ) {
    autoResumeRef.current = activeSession.id;
    queueMicrotask(() => void handleStartShell(activeSession.id));
  }

  if (relevantSessions.length === 0 && !activeTab) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '12px 16px',
            borderBottom: '1px solid var(--border)',
            flexShrink: 0,
          }}
        >
          <div>
            <div style={{ fontSize: 14, fontWeight: 500, color: 'var(--text-1)' }}>{label}</div>
            <div style={{ fontSize: 11, color: 'var(--text-3)', marginTop: 2 }}>{cwd}</div>
          </div>
        </div>
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 12,
          }}
        >
          <div style={{ display: 'flex', gap: 8 }}>
            <button className="btn btn-primary" onClick={() => void handleNewTerminal('claude')}>
              + Claude terminal
            </button>
            <button className="btn btn-secondary" onClick={() => void handleNewTerminal('codex')}>
              + Codex terminal
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '8px 16px',
          borderBottom: '1px solid var(--border)',
          flexShrink: 0,
        }}
      >
        <div style={{ fontSize: 13, fontWeight: 500, color: 'var(--text-1)' }}>{label}</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {onClose && (
            <button
              onClick={onClose}
              style={{
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                color: 'var(--text-3)',
                padding: 4,
              }}
              title="Close terminal"
            >
              <X size={14} />
            </button>
          )}
          {onAdopt && (
            <button
              className="btn btn-secondary"
              onClick={handleAdopt}
              disabled={adopting}
              style={{ fontSize: 12, display: 'flex', alignItems: 'center', gap: 4 }}
            >
              <ArrowRight size={12} />
              {adopting ? 'Adopting...' : 'Adopt'}
            </button>
          )}
        </div>
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          borderBottom: '1px solid var(--border)',
          paddingLeft: 8,
          flexShrink: 0,
          height: 32,
          fontSize: 12,
        }}
      >
        {relevantSessions.map((session) => (
          <div
            key={session.id}
            onClick={() => setActiveTab(session.id)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '0 10px',
              height: '100%',
              cursor: 'pointer',
              borderBottom:
                activeTab === session.id ? '2px solid var(--accent)' : '2px solid transparent',
              color: activeTab === session.id ? 'var(--text-1)' : 'var(--text-3)',
            }}
          >
            <Circle
              size={6}
              fill={
                session.running
                  ? 'var(--green)'
                  : isRestoredTerminalSession(session)
                    ? 'var(--accent)'
                    : 'var(--text-4)'
              }
              stroke="none"
            />
            <span>
              {session.agent} {session.id.slice(0, 6)}
            </span>
            <X
              size={12}
              style={{ opacity: 0.5, cursor: 'pointer' }}
              onClick={(event) => {
                event.stopPropagation();
                void handleCloseTab(session.id);
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

      <div style={{ flex: 1, minHeight: 0 }}>
        {activeSession && (
          <TerminalView
            key={activeSession.id}
            session={activeSession}
            onExit={(code) => {
              setExitStates((prev) => ({
                ...prev,
                [activeSession.id]: { running: false, exit_code: code },
              }));
              queryClient.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
                old?.map((session) =>
                  session.id === activeSession.id
                    ? { ...session, running: false, exit_code: code }
                    : session,
                ),
              );
            }}
          />
        )}
      </div>
    </div>
  );
}
