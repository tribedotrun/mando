import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { isRestoredTerminalSession } from '#renderer/domains/terminal/runtime/terminalSession';
import { useTerminalList, useConfig, type TerminalSessionInfo } from '#renderer/hooks/queries';
import { useTerminalCreate, useTerminalDelete } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import { X, Plus, Circle, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import log from '#renderer/logger';

interface TerminalPageProps {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
  resumeName?: string | null;
  onResumeConsumed?: () => void;
}

const AGENTS = [
  { id: 'claude' as const, label: 'claude', icon: '*' },
  { id: 'codex' as const, label: 'codex', icon: '@' },
];

export function TerminalPage({
  project,
  cwd,
  resumeSessionId,
  resumeName,
  onResumeConsumed,
}: TerminalPageProps) {
  const { data: sessions = [], isSuccess: sessionsLoaded } = useTerminalList();
  const createMutation = useTerminalCreate();
  const deleteMutation = useTerminalDelete();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [startingShellForId, setStartingShellForId] = useState<string | null>(null);
  const [exitStates, setExitStates] = useState<
    Record<string, { running: boolean; exit_code: number | null }>
  >({});
  const [resumePending, setResumePending] = useState(!!resumeSessionId);
  const [resumeFailed, setResumeFailed] = useState(false);
  const initRef = useRef(false);

  const { data: config } = useConfig();
  const defaultAgent = config?.captain?.defaultTerminalAgent ?? 'claude';

  const sessionsWithExitState = sessions.map((session) => {
    const override = exitStates[session.id];
    return override ? { ...session, ...override } : session;
  });

  const relevantSessions = sessionsWithExitState.filter(
    (session) => session.project === project && session.cwd === cwd,
  );

  const autoSelectedRef = useRef(false);

  const handleNewTerminal = useCallback(
    async (agent: 'claude' | 'codex') => {
      setResumeFailed(false);
      autoSelectedRef.current = true;
      try {
        const session = await createMutation.mutateAsync({ project, cwd, agent });
        setActiveTab(session.id);
      } catch (err) {
        log.error('Failed to create terminal', err);
        toast.error(err instanceof Error ? err.message : 'Failed to create terminal');
      }
    },
    [createMutation, cwd, project],
  );

  useMountEffect(() => {
    if (initRef.current) return;
    initRef.current = true;

    if (resumeSessionId) {
      setResumePending(true);
      void (async () => {
        try {
          const session = await createMutation.mutateAsync({
            project,
            cwd,
            agent: 'claude',
            resume_session_id: resumeSessionId,
            name: resumeName ?? undefined,
          });
          setResumePending(false);
          setActiveTab(session.id);
          onResumeConsumed?.();
        } catch (err) {
          setResumePending(false);
          setResumeFailed(true);
          log.error('Failed to resume terminal session', err);
          onResumeConsumed?.();
        }
      })();
    }
  });

  if (
    !autoSelectedRef.current &&
    !activeTab &&
    !resumeSessionId &&
    !resumePending &&
    !resumeFailed &&
    sessionsLoaded
  ) {
    const relevant = sessions.filter(
      (session) => session.project === project && session.cwd === cwd,
    );
    if (relevant.length > 0) {
      autoSelectedRef.current = true;
      setActiveTab(relevant[relevant.length - 1].id);
    } else if (sessions.length === 0 || relevant.length === 0) {
      autoSelectedRef.current = true;
      queueMicrotask(() => void handleNewTerminal(defaultAgent));
    }
  }

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
        },
      );
    },
    [activeTab, deleteMutation, relevantSessions],
  );

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
          ...(session.agent === 'claude' ? { resume_session_id: session.ccSessionId ?? '' } : {}),
        });
        setActiveTab(next.id);
        await deleteMutation.mutateAsync({ id: sessionId });
      } catch (err) {
        log.error('Failed to replace restored terminal', err);
        toast.error(err instanceof Error ? err.message : 'Failed to start terminal');
      } finally {
        setStartingShellForId((current) => (current === sessionId ? null : current));
      }
    },
    [createMutation, deleteMutation, relevantSessions],
  );

  const handleExit = useCallback(
    (id: string, code: number | null) => {
      setExitStates((prev) => ({ ...prev, [id]: { running: false, exit_code: code } }));
      queryClient.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old?.map((session) =>
          session.id === id ? { ...session, running: false, exit_code: code } : session,
        ),
      );
    },
    [queryClient],
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

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="flex shrink-0 items-center px-4" style={{ height: 36 }}>
        {relevantSessions.map((session) => (
          <div
            key={session.id}
            onClick={() => setActiveTab(session.id)}
            className="flex cursor-pointer items-center gap-1.5 px-3"
            style={{
              height: '100%',
              borderBottom:
                activeTab === session.id
                  ? '2px solid var(--color-accent)'
                  : '2px solid transparent',
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
            <span>{session.name || `${session.agent} ${session.id.slice(0, 6)}`}</span>
            <X
              size={12}
              className="opacity-40 hover:opacity-100"
              style={{ cursor: 'pointer' }}
              onClick={(event) => {
                event.stopPropagation();
                void handleCloseTab(session.id);
              }}
            />
          </div>
        ))}

        {relevantSessions.length > 0 && <div className="mx-2 h-4 border-l border-border-subtle" />}

        {AGENTS.map((agent) => (
          <button
            key={agent.id}
            onClick={() => void handleNewTerminal(agent.id)}
            className="flex cursor-pointer items-center gap-1.5 rounded px-2.5 py-1 text-caption text-text-3 transition-colors hover:bg-surface-2 hover:text-text-2"
            style={{ background: 'none', border: 'none' }}
          >
            <Plus size={10} />
            {agent.label}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1">
        {activeSession ? (
          <TerminalView
            key={activeSession.id}
            session={activeSession}
            onExit={(code) => handleExit(activeSession.id, code)}
          />
        ) : resumePending ? (
          <div className="flex h-full items-center justify-center gap-2 text-caption text-text-3">
            <Loader2 size={14} className="animate-spin" />
            Resuming session...
          </div>
        ) : resumeFailed ? (
          <div className="flex h-full items-center justify-center text-caption text-text-3">
            Session resume failed. Start a new terminal to continue.
          </div>
        ) : (
          <div className="flex h-full items-center justify-center text-caption text-text-3">
            Select an agent above to start a terminal
          </div>
        )}
      </div>
    </div>
  );
}
