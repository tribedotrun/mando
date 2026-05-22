import React from 'react';
import { useWorkbenchPage } from '#renderer/domains/captain';
import { TaskDetailView } from '#renderer/domains/captain/ui/TaskDetailView';
import { TerminalPage } from '#renderer/domains/captain/terminal/ui/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/captain/terminal/ui/WorkspacePreparing';
import { WorktreeMissing } from '#renderer/domains/captain/terminal/ui/WorktreeMissing';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';

export function WorkbenchPage(): React.ReactElement {
  const page = useWorkbenchPage();

  // New workbench creation flow. The URL (`workbenchId === 'new'`) is the
  // single source of truth: useWorkbenchPage's render-body re-entry block
  // re-fires `openNewTerminal` on every workbenchId/project transition, so
  // there is no longer a "preparing flag is false but URL is /wb/new" gap
  // that previously fell through to a silent dead text. On failure
  // useWorkbenchPage navigates back to '/' rather than leaving the user
  // stuck here.
  if (page.ids.isNewWorkbench) {
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

  // Worktree missing: replace the terminal surface with a clear actionable
  // banner instead of letting TerminalPage mount, fail to start a session at
  // a deleted cwd, and render a misleading `[Process exited with code N]`
  // from a stale prior exit. Fail-closed semantics: only the daemon's
  // affirmative `false` triggers the missing surface — an absent or
  // `undefined` value falls through to the regular terminal.
  const wb = page.data.workbench;
  const worktreeMissing = wb.worktreeExists === false;
  const missingNode = <WorktreeMissing workbenchId={wb.id} worktree={wb.worktree} />;

  // Taskless workbench: render terminal directly
  if (!page.data.task) {
    return (
      <div className="h-full px-3 pt-2">
        <ErrorBoundary fallbackLabel="Terminal">
          {worktreeMissing ? (
            missingNode
          ) : (
            <TerminalPage
              key={`terminal-${wb.id}`}
              workbenchId={wb.id}
              project={wb.project}
              cwd={wb.worktree}
              resumeSessionId={page.search.resume}
              resumeName={page.search.name}
              onResumeConsumed={page.nav.handleResumeConsumed}
            />
          )}
        </ErrorBoundary>
      </div>
    );
  }

  // Task workbench: lazy-mount terminal only after user visits the terminal tab.
  // This prevents eagerly creating terminal sessions on every task navigation.
  const terminalSlot = page.nav.terminalVisited ? (
    worktreeMissing ? (
      missingNode
    ) : (
      <TerminalPage
        key={`terminal-${wb.id}`}
        workbenchId={wb.id}
        project={wb.project}
        cwd={wb.worktree}
        resumeSessionId={page.search.tab === 'terminal' ? (page.search.resume ?? null) : null}
        resumeName={page.search.tab === 'terminal' ? (page.search.name ?? null) : null}
        onResumeConsumed={page.nav.handleResumeConsumed}
      />
    )
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
