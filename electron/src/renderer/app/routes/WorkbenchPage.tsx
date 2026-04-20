import React from 'react';
import { useWorkbenchPage } from '#renderer/domains/captain';
import { TaskDetailView } from '#renderer/domains/captain/ui/TaskDetailView';
import { TerminalPage } from '#renderer/domains/captain/terminal/ui/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/captain/terminal/ui/WorkspacePreparing';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';

export function WorkbenchPage(): React.ReactElement {
  const {
    isNewWorkbench,
    search,
    terminalKey,
    terminalPage,
    workbench,
    task,
    workbenchesLoading,
    tasksLoading,
    terminalVisited,
    handleBack,
    handleOpenTranscript,
    handleTabChange,
    handleResumeInTerminal,
    handleResumeConsumed,
    handleCancelNew,
  } = useWorkbenchPage();

  // New workbench creation flow
  if (isNewWorkbench) {
    if (terminalPage?.preparing) {
      return (
        <div className="h-full px-3 pt-2">
          <ErrorBoundary fallbackLabel="Workspace preparing">
            <WorkspacePreparing project={search.project ?? ''} onCancel={handleCancelNew} />
          </ErrorBoundary>
        </div>
      );
    }
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        Preparing workspace...
      </div>
    );
  }

  // Existing workbench
  if (!workbench) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {tasksLoading || workbenchesLoading ? 'Loading...' : 'Workbench not found'}
      </div>
    );
  }

  // Taskless workbench: render terminal directly
  if (!task) {
    return (
      <div className="h-full px-3 pt-2">
        <ErrorBoundary fallbackLabel="Terminal">
          <TerminalPage
            key={`terminal-${workbench.id}-${terminalKey}`}
            project={workbench.project}
            cwd={workbench.worktree}
            resumeSessionId={search.resume}
            resumeName={search.name}
            onResumeConsumed={handleResumeConsumed}
          />
        </ErrorBoundary>
      </div>
    );
  }

  // Task workbench: lazy-mount terminal only after user visits the terminal tab.
  // This prevents eagerly creating terminal sessions on every task navigation.
  const terminalSlot = terminalVisited ? (
    <TerminalPage
      key={`terminal-${workbench.id}-${terminalKey}`}
      project={workbench.project}
      cwd={workbench.worktree}
      resumeSessionId={search.tab === 'terminal' ? (search.resume ?? null) : null}
      resumeName={search.tab === 'terminal' ? (search.name ?? null) : null}
      onResumeConsumed={handleResumeConsumed}
    />
  ) : null;

  return (
    <div className="h-full px-3 pt-2">
      <ErrorBoundary fallbackLabel="Workbench">
        <TaskDetailView
          item={task}
          onBack={handleBack}
          onOpenTranscript={handleOpenTranscript}
          activeTab={search.tab}
          onTabChange={handleTabChange}
          onResumeInTerminal={handleResumeInTerminal}
          terminalSlot={terminalSlot}
        />
      </ErrorBoundary>
    </div>
  );
}
