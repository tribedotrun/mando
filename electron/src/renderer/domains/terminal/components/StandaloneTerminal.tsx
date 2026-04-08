import { useCallback, useState } from 'react';
import { TerminalView } from '#renderer/domains/terminal/components/TerminalView';
import { useTerminalStore } from '#renderer/domains/terminal/stores/terminalStore';
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
  const { sessions, addSession, removeSession, updateSession } = useTerminalStore();
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [adopting, setAdopting] = useState(false);

  const relevantSessions = sessions.filter((s) => s.project === project && s.cwd === cwd);

  const handleNewTerminal = useCallback(
    async (agent: 'claude' | 'codex') => {
      const session = await addSession({ project, cwd, agent });
      setActiveTab(session.id);
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

  const handleAdopt = useCallback(() => {
    setAdopting(true);
    onAdopt?.(cwd);
  }, [cwd, onAdopt]);

  // Auto-create first terminal if none exist.
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
      {/* Header */}
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

      {/* Terminal sub-tabs */}
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
      <div style={{ flex: 1, minHeight: 0 }}>
        {activeTab && (
          <TerminalView
            key={activeTab}
            sessionId={activeTab}
            onExit={(code) => updateSession(activeTab, { running: false, exit_code: code })}
          />
        )}
      </div>
    </div>
  );
}
