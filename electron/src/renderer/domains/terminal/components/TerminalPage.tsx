import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { useTerminalList, useConfig, type TerminalSessionInfo } from '#renderer/hooks/queries';
import { useTerminalCreate, useTerminalDelete } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import { X, Plus, Circle } from 'lucide-react';
import { toast } from 'sonner';
import log from '#renderer/logger';

interface TerminalPageProps {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
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
  onResumeConsumed,
}: TerminalPageProps) {
  const { data: sessions = [] } = useTerminalList();
  const createMutation = useTerminalCreate();
  const deleteMutation = useTerminalDelete();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [exitStates, setExitStates] = useState<
    Record<string, { running: boolean; exit_code: number | null }>
  >({});
  const initRef = useRef(false);

  const { data: _cfg } = useConfig();
  const defaultAgent = _cfg?.captain?.defaultTerminalAgent ?? 'claude';

  // Merge exit states into sessions for rendering.
  const sessionsWithExitState = sessions.map((s) => {
    const override = exitStates[s.id];
    return override ? { ...s, ...override } : s;
  });

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

  // Init: resume or auto-select/create on mount once sessions are loaded.
  useMountEffect(() => {
    if (initRef.current) return;
    initRef.current = true;

    if (resumeSessionId) {
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
    }
  });

  // Once sessions load, auto-select last relevant or create a new one (render-time).
  const autoSelectedRef = useRef(false);
  if (!autoSelectedRef.current && !activeTab && !resumeSessionId && sessions.length > 0) {
    const relevant = sessions.filter((s) => s.project === project && s.cwd === cwd);
    autoSelectedRef.current = true;
    if (relevant.length > 0) {
      setActiveTab(relevant[relevant.length - 1].id);
    } else {
      // Cannot call async handler during render; schedule for next microtask.
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
              const remaining = relevantSessions.filter((s) => s.id !== id);
              setActiveTab(remaining.length > 0 ? remaining[0].id : null);
            }
          },
        },
      );
    },
    [activeTab, relevantSessions, deleteMutation],
  );

  const handleExit = useCallback(
    (id: string, code: number | null) => {
      setExitStates((prev) => ({ ...prev, [id]: { running: false, exit_code: code } }));
      queryClient.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), (old) =>
        old?.map((s) => (s.id === id ? { ...s, running: false, exit_code: code } : s)),
      );
    },
    [queryClient],
  );

  return (
    <div className="flex h-full flex-col bg-bg">
      {/* Terminal sub-tabs + agent presets */}
      <div className="flex shrink-0 items-center px-4" style={{ height: 36 }}>
        {/* Open session tabs */}
        {relevantSessions.map((s) => (
          <div
            key={s.id}
            onClick={() => setActiveTab(s.id)}
            className="flex cursor-pointer items-center gap-1.5 px-3"
            style={{
              height: '100%',
              borderBottom:
                activeTab === s.id ? '2px solid var(--color-accent)' : '2px solid transparent',
              color: activeTab === s.id ? 'var(--color-text-1)' : 'var(--color-text-3)',
              fontSize: 13,
            }}
          >
            <Circle
              size={6}
              fill={s.running ? 'var(--color-success)' : 'var(--color-text-4)'}
              stroke="none"
            />
            <span>
              {s.agent} {s.id.slice(0, 6)}
            </span>
            <X
              size={12}
              className="opacity-40 hover:opacity-100"
              style={{ cursor: 'pointer' }}
              onClick={(e) => {
                e.stopPropagation();
                void handleCloseTab(s.id);
              }}
            />
          </div>
        ))}

        {/* Separator */}
        {relevantSessions.length > 0 && <div className="mx-2 h-4 border-l border-border-subtle" />}

        {/* Agent presets */}
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

      {/* Terminal content */}
      <div className="min-h-0 flex-1">
        {activeTab ? (
          <TerminalView
            key={activeTab}
            sessionId={activeTab}
            onExit={(code) => handleExit(activeTab, code)}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-caption text-text-3">
            Select an agent above to start a terminal
          </div>
        )}
      </div>
    </div>
  );
}
