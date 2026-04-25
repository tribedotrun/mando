import { useCallback, useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useTerminalCache } from '#renderer/domains/captain/runtime/useTerminalCache';
import { isRestoredTerminalSession } from '#renderer/domains/captain/terminal/runtime/terminalSession';
import {
  useTerminalList,
  useTerminalCreate,
  useTerminalDelete,
} from '#renderer/domains/captain/runtime/hooks';
import { useConfig } from '#renderer/global/repo/queries';
import { toast } from '#renderer/global/runtime/useFeedback';
import log from '#renderer/global/service/logger';

interface OrchestrationInput {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
  resumeName?: string | null;
  onResumeConsumed?: () => void;
}

export function useTerminalOrchestration({
  project,
  cwd,
  resumeSessionId,
  resumeName,
  onResumeConsumed,
}: OrchestrationInput) {
  const { data: sessions = [], isSuccess: sessionsLoaded } = useTerminalList();
  const createMutation = useTerminalCreate();
  const deleteMutation = useTerminalDelete();
  const { getTerminals, setTerminals } = useTerminalCache();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [startingShellForId, setStartingShellForId] = useState<string | null>(null);
  const [exitStates, setExitStates] = useState<
    Record<string, { running: boolean; exit_code: number | null }>
  >({});
  const [resumePending, setResumePending] = useState(!!resumeSessionId);
  const [resumeFailed, setResumeFailed] = useState(false);
  const initRef = useRef(false);
  const autoSelectedRef = useRef(false);
  const autoResumeRef = useRef<string | null>(null);

  const { data: config } = useConfig();
  const defaultAgent = config?.captain?.defaultTerminalAgent ?? 'claude';

  const sessionsWithExitState = sessions.map((session) => {
    const override = exitStates[session.id];
    return override ? { ...session, ...override } : session;
  });

  const relevantSessions = sessionsWithExitState.filter(
    (session) => session.project === project && session.cwd === cwd,
  );

  // -- Handlers ---------------------------------------------------------------

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

      autoSelectedRef.current = true;
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
      setTerminals((old) =>
        (old ?? []).map((session) =>
          session.id === id ? { ...session, running: false, exit_code: code } : session,
        ),
      );
    },
    [setTerminals],
  );

  // -- Mount: resume flow -----------------------------------------------------

  useMountEffect(() => {
    if (initRef.current) return;
    initRef.current = true;

    if (resumeSessionId) {
      setResumePending(true);
      autoSelectedRef.current = true;

      const cached = getTerminals() ?? [];
      const prior = cached.filter((s) => s.project === project && s.cwd === cwd);
      const autoCreatedId = prior.length === 1 ? prior[0].id : null;

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

          if (autoCreatedId && autoCreatedId !== session.id) {
            deleteMutation.mutate({ id: autoCreatedId });
          }
        } catch (err) {
          setResumePending(false);
          setResumeFailed(true);
          log.error('Failed to resume terminal session', err);
          onResumeConsumed?.();
        }
      })();
    }
  });

  // -- Auto-select / auto-create ----------------------------------------------

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

  // -- Auto-resume restored sessions -----------------------------------------

  const activeSession = activeTab
    ? (relevantSessions.find((session) => session.id === activeTab) ?? null)
    : null;

  if (
    activeSession &&
    isRestoredTerminalSession(activeSession) &&
    autoResumeRef.current !== activeSession.id &&
    !startingShellForId
  ) {
    autoResumeRef.current = activeSession.id;
    queueMicrotask(() => void handleStartShell(activeSession.id));
  }

  return {
    sessions: { relevantSessions, activeSession },
    tabs: { activeTab, setActiveTab },
    resume: { pending: resumePending, failed: resumeFailed },
    actions: { handleNewTerminal, handleCloseTab, handleExit },
  };
}
