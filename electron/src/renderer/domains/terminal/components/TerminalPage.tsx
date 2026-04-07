import { useCallback, useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { useTerminalStore } from '#renderer/domains/terminal/store';
import { X, Plus, Circle, ArrowLeft } from 'lucide-react';
import { toast } from 'sonner';
import log from '#renderer/logger';

interface TerminalPageProps {
  project: string;
  cwd: string;
  label: string;
  onBack: () => void;
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
  label,
  onBack,
  resumeSessionId,
  onResumeConsumed,
}: TerminalPageProps) {
  const {
    sessions,
    addSession,
    removeSession,
    updateSession,
    fetch: fetchSessions,
  } = useTerminalStore();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const resumedRef = useRef(false);

  useMountEffect(() => {
    fetchSessions().then(() => {
      const store = useTerminalStore.getState();
      const relevant = store.sessions.filter((s) => s.project === project && s.cwd === cwd);
      if (relevant.length > 0 && !activeTab) {
        setActiveTab(relevant[relevant.length - 1].id);
      }
    });
  });

  useMountEffect(() => {
    if (!resumeSessionId || resumedRef.current) return;
    resumedRef.current = true;
    addSession({ project, cwd, agent: 'claude', resume_session_id: resumeSessionId })
      .then((session) => {
        setActiveTab(session.id);
        onResumeConsumed?.();
      })
      .catch((e) => log.error('Failed to resume terminal session', e));
  });

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
    async (id: string) => {
      await removeSession(id);
      if (activeTab === id) {
        const remaining = relevantSessions.filter((s) => s.id !== id);
        setActiveTab(remaining.length > 0 ? remaining[0].id : null);
      }
    },
    [activeTab, relevantSessions, removeSession],
  );

  const handleExit = useCallback(
    (id: string, code: number | null) => {
      updateSession(id, { running: false, exit_code: code });
    },
    [updateSession],
  );

  return (
    <div className="flex h-full flex-col bg-bg">
      {/* Header: back + label + agent presets */}
      <div className="flex shrink-0 items-center gap-3 px-4 pt-2 pb-1">
        <button
          onClick={onBack}
          className="flex items-center gap-1 text-caption text-text-3 hover:text-text-2"
          style={{ background: 'none', border: 'none', cursor: 'pointer' }}
        >
          <ArrowLeft size={14} />
        </button>
        <span className="text-body font-medium text-text-1">{label}</span>
        <span className="flex-1" />
        <span className="text-caption text-text-3">{cwd}</span>
      </div>

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
                handleCloseTab(s.id);
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
            onClick={() => handleNewTerminal(agent.id)}
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
