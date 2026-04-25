import { useTerminalOrchestration } from '#renderer/domains/captain/terminal/runtime/useTerminalOrchestration';
import { TerminalView } from '#renderer/domains/captain/terminal/ui/TerminalView';
import { TerminalTabBar } from '#renderer/domains/captain/terminal/ui/TerminalTabBar';
import { Loader2 } from 'lucide-react';

interface TerminalPageProps {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
  resumeName?: string | null;
  onResumeConsumed?: () => void;
}

export function TerminalPage({
  project,
  cwd,
  resumeSessionId,
  resumeName,
  onResumeConsumed,
}: TerminalPageProps) {
  const terminal = useTerminalOrchestration({
    project,
    cwd,
    resumeSessionId,
    resumeName,
    onResumeConsumed,
  });
  const activeSession = terminal.sessions.activeSession;

  return (
    <div className="flex h-full flex-col bg-bg">
      <TerminalTabBar
        sessions={terminal.sessions.relevantSessions}
        activeTab={terminal.tabs.activeTab}
        onSelectTab={terminal.tabs.setActiveTab}
        onCloseTab={(id) => void terminal.actions.handleCloseTab(id)}
        onNewTerminal={(agent) => void terminal.actions.handleNewTerminal(agent)}
      />

      <div className="min-h-0 flex-1">
        {activeSession ? (
          <TerminalView
            key={activeSession.id}
            session={activeSession}
            onExit={(code) => terminal.actions.handleExit(activeSession.id, code)}
          />
        ) : terminal.resume.pending ? (
          <div className="flex h-full items-center justify-center gap-2 text-caption text-text-3">
            <Loader2 size={14} className="animate-spin" />
            Resuming session...
          </div>
        ) : terminal.resume.failed ? (
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
