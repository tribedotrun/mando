import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTerminalCache } from '#renderer/domains/captain/runtime/useTerminalCache';
import {
  isRestoredTerminalSession,
  selectWorkbenchTerminalSessions,
} from '#renderer/domains/captain/terminal/runtime/terminalSession';
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
  /**
   * Additional acceptable cwds for this workbench. The clarifier's
   * cc_sessions row stores cwd = project root rather than worktree, so
   * resuming it lands a terminal session whose cwd does not match
   * `workbench.worktree`. Pass the project root here so those resumed
   * clarifier terminals still appear in this workbench's tab bar.
   */
  extraCwds?: readonly string[];
  resumeSessionId?: string | null;
  resumeName?: string | null;
  onResumeConsumed?: () => void;
}

export function useTerminalOrchestration({
  project,
  cwd,
  extraCwds,
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
  const autoSelectedRef = useRef(false);
  const autoResumeRef = useRef<string | null>(null);
  /**
   * Terminal ids the orchestration auto-spawned on first mount (the empty-
   * workbench branch). Only those are evictable when a resume completes.
   *
   * Explicit user "+ Claude" / "+ Codex" tab-bar clicks go through
   * `handleNewTerminal` and intentionally stay OUT of this set — a tab the
   * user opened on purpose must survive a subsequent Resume click.
   *
   * Replaces the `cached.length === 1` heuristic that incorrectly evicted a
   * previously-resumed terminal as soon as a second resume request arrived.
   */
  const blankIdsRef = useRef<Set<string>>(new Set());
  /**
   * Last `resumeSessionId` we acted on. After `onResumeConsumed` clears the
   * URL the prop transitions back to null and we reset this ref, so a repeat
   * click on the same session id reads as a fresh request rather than a
   * silent no-op (Bug 1).
   */
  const lastResumeRef = useRef<string | null>(null);
  /**
   * Monotonic counter so an in-flight resume that gets superseded by a newer
   * resume request resolves into a no-op instead of clobbering the newer
   * request's pending/active state.
   */
  const resumeGenRef = useRef(0);

  const { data: config } = useConfig();
  const defaultAgent = config?.captain?.defaultTerminalAgent ?? 'claude';

  const acceptedCwds = useMemo(() => {
    const extras = extraCwds?.filter((c) => c && c !== cwd) ?? [];
    return [cwd, ...extras];
  }, [cwd, extraCwds]);

  const sessionsWithExitState = sessions.map((session) => {
    const override = exitStates[session.id];
    return override ? { ...session, ...override } : session;
  });

  const relevantSessions = selectWorkbenchTerminalSessions(
    sessionsWithExitState,
    project,
    acceptedCwds,
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

  // Empty-workbench auto-create. Distinct from `handleNewTerminal` because
  // the resulting tab is registered as evictable: when the user later
  // resumes a session, we delete this auto-spawned blank to avoid
  // littering the tab bar with an empty terminal nobody asked for. Tabs
  // the user explicitly opens via `+ Claude`/`+ Codex` go through
  // `handleNewTerminal` and stay out of `blankIdsRef`.
  const autoCreateBlank = useCallback(
    async (agent: 'claude' | 'codex') => {
      setResumeFailed(false);
      autoSelectedRef.current = true;
      try {
        const session = await createMutation.mutateAsync({ project, cwd, agent });
        blankIdsRef.current.add(session.id);
        setActiveTab(session.id);
      } catch (err) {
        log.error('Failed to auto-create terminal', err);
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
            blankIdsRef.current.delete(id);
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

  // -- Resume flow ------------------------------------------------------------

  const startResume = useCallback(
    (sessionId: string, displayName: string | null | undefined) => {
      const myGen = ++resumeGenRef.current;
      setResumePending(true);
      setResumeFailed(false);
      autoSelectedRef.current = true;

      void (async () => {
        try {
          const session = await createMutation.mutateAsync({
            project,
            cwd,
            agent: 'claude',
            resume_session_id: sessionId,
            name: displayName ?? undefined,
          });
          if (resumeGenRef.current !== myGen) return;
          setResumePending(false);
          setActiveTab(session.id);
          onResumeConsumed?.();

          // Evict an auto-created blank — only ones THIS orchestration
          // spawned and still match the workbench. Computed AFTER the
          // resume completes so any parallel auto-create has had a chance
          // to register itself in `blankIdsRef`. Triggers when
          // `handleResumeInTerminal` toggles `terminalVisited` and updates
          // the URL in the same render but the search params land a tick
          // later, so the orchestration may briefly observe
          // `resumeSessionId === null` and spawn a blank before the resume
          // id arrives.
          const cached = getTerminals() ?? [];
          const acceptedSet = new Set(acceptedCwds);
          let blankToDelete: string | null = null;
          for (const id of blankIdsRef.current) {
            if (id === session.id) continue;
            const match = cached.find(
              (s) => s.id === id && s.project === project && acceptedSet.has(s.cwd),
            );
            if (match) {
              blankToDelete = id;
              break;
            }
          }
          if (blankToDelete) {
            deleteMutation.mutate({ id: blankToDelete });
            blankIdsRef.current.delete(blankToDelete);
          }
        } catch (err) {
          if (resumeGenRef.current !== myGen) return;
          setResumePending(false);
          setResumeFailed(true);
          log.error('Failed to resume terminal session', err);
          onResumeConsumed?.();
        }
      })();
    },
    [acceptedCwds, createMutation, cwd, deleteMutation, getTerminals, onResumeConsumed, project],
  );

  // Reactive resume: fire a fresh attempt every time the URL prop
  // transitions to a non-null id we have not yet processed. Reset when
  // the prop clears so a repeat click on the same id re-fires (after
  // `onResumeConsumed` clears the URL the prop becomes null and the
  // ref clears, so the next click reads as a new request).
  //
  // `startResume` is held by ref so the effect deps stay narrow and an
  // unrelated re-render (project, cwd, accepted-cwds identity, etc.)
  // does not re-trigger. Strict-Mode double-invocation of effects is
  // safe because `lastResumeRef` guards the second pass.
  const startResumeRef = useRef(startResume);
  startResumeRef.current = startResume;

  useEffect(() => {
    if (!resumeSessionId) {
      lastResumeRef.current = null;
      return;
    }
    if (lastResumeRef.current === resumeSessionId) return;
    lastResumeRef.current = resumeSessionId;
    startResumeRef.current(resumeSessionId, resumeName ?? null);
  }, [resumeSessionId, resumeName]);

  // -- Auto-select / auto-create ----------------------------------------------

  if (
    !autoSelectedRef.current &&
    !activeTab &&
    !resumeSessionId &&
    !resumePending &&
    !resumeFailed &&
    sessionsLoaded
  ) {
    const relevant = selectWorkbenchTerminalSessions(sessions, project, acceptedCwds);
    if (relevant.length > 0) {
      autoSelectedRef.current = true;
      setActiveTab(relevant[relevant.length - 1].id);
    } else if (sessions.length === 0 || relevant.length === 0) {
      autoSelectedRef.current = true;
      queueMicrotask(() => void autoCreateBlank(defaultAgent));
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
