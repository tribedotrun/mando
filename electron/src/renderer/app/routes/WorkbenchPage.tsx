import React from 'react';
import { useWorkbenchPage } from '#renderer/domains/captain';
import { TaskDetailView } from '#renderer/domains/captain/ui/TaskDetailView';
import { WorkbenchWithoutTask } from '#renderer/domains/captain/ui/WorkbenchWithoutTask';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';

export function WorkbenchPage(): React.ReactElement {
  const page = useWorkbenchPage();

  if (!page.data.workbench) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {page.data.tasksLoading || page.data.workbenchesLoading
          ? 'Loading...'
          : 'Workbench not found'}
      </div>
    );
  }

  // A workbench without a task has nothing to render. Keep the loading shimmer
  // while the task lookup is unresolved (cold reload, or a task that lives in
  // the archived list) so the page never flashes the empty state first.
  if (!page.data.task) {
    if (page.data.tasksLoading) {
      return (
        <div className="flex h-full items-center justify-center text-muted-foreground">
          Loading...
        </div>
      );
    }
    return (
      <WorkbenchWithoutTask workbenchId={page.data.workbench.id} onArchived={page.nav.handleBack} />
    );
  }

  return (
    <div className="h-full px-3 pt-2">
      <ErrorBoundary fallbackLabel="Workbench">
        <TaskDetailView
          item={page.data.task}
          onBack={page.nav.handleBack}
          onOpenTranscript={page.nav.handleOpenTranscript}
          activeTab={page.search.tab}
          onTabChange={page.nav.handleTabChange}
        />
      </ErrorBoundary>
    </div>
  );
}
