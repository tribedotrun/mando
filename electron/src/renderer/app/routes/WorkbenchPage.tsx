import React from 'react';
import { useWorkbenchPage } from '#renderer/domains/captain';
import { TaskDetailView } from '#renderer/domains/captain/ui/TaskDetailView';
import { TerminalPage } from '#renderer/domains/captain/terminal/ui/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/captain/terminal/ui/WorkspacePreparing';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';

export function WorkbenchPage(): React.ReactElement {
  const page = useWorkbenchPage();

  // New workbench creation flow
  if (page.ids.isNewWorkbench) {
    if (page.terminal.page?.preparing) {
      return (
        <div className="h-full px-3 pt-2">
          <ErrorBoundary fallbackLabel="Workspace preparing">
            <WorkspacePreparing
              project={page.search.project ?? ''}
              onCancel={page.actions.handleCancelNew}
            />
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
  if (!page.data.workbench) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {page.data.tasksLoading || page.data.workbenchesLoading
          ? 'Loading...'
          : 'Workbench not found'}
      </div>
    );
  }

  // Task lookup unresolved: keep the loading shimmer instead of falling through
  // to the taskless terminal branch. Without this, a workbench whose task is
  // still loading (cold reload) or lives in the archived task list (auto-archived
  // workbench) renders headerless before the task resolves a frame later.
  if (!page.data.task && page.data.tasksLoading) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  // Taskless workbench: render terminal directly
  if (!page.data.task) {
    return (
      <div className="h-full px-3 pt-2">
        <ErrorBoundary fallbackLabel="Terminal">
          <TerminalPage
            key={`terminal-${page.data.workbench.id}-${page.terminal.key}`}
            project={page.data.workbench.project}
            cwd={page.data.workbench.worktree}
            resumeSessionId={page.search.resume}
            resumeName={page.search.name}
            onResumeConsumed={page.nav.handleResumeConsumed}
          />
        </ErrorBoundary>
      </div>
    );
  }

  // Task workbench: lazy-mount terminal only after user visits the terminal tab.
  // This prevents eagerly creating terminal sessions on every task navigation.
  const terminalSlot = page.nav.terminalVisited ? (
    <TerminalPage
      key={`terminal-${page.data.workbench.id}-${page.terminal.key}`}
      project={page.data.workbench.project}
      cwd={page.data.workbench.worktree}
      resumeSessionId={page.search.tab === 'terminal' ? (page.search.resume ?? null) : null}
      resumeName={page.search.tab === 'terminal' ? (page.search.name ?? null) : null}
      onResumeConsumed={page.nav.handleResumeConsumed}
    />
  ) : null;

  return (
    <div className="h-full px-3 pt-2">
      <ErrorBoundary fallbackLabel="Workbench">
        <TaskDetailView
          item={page.data.task}
          onBack={page.nav.handleBack}
          onOpenTranscript={page.nav.handleOpenTranscript}
          activeTab={page.search.tab}
          onTabChange={page.nav.handleTabChange}
          onResumeInTerminal={page.nav.handleResumeInTerminal}
          terminalSlot={terminalSlot}
        />
      </ErrorBoundary>
    </div>
  );
}
