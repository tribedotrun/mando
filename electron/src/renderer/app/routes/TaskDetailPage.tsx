import React, { useCallback } from 'react';
import { useNavigate, useParams } from '@tanstack/react-router';
import { useTaskList } from '#renderer/hooks/queries';
import { useUIStore } from '#renderer/app/uiStore';
import { TaskDetailView } from '#renderer/domains/captain/components/TaskDetailView';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function TaskDetailPage(): React.ReactElement {
  const navigate = useNavigate();
  const { taskId } = useParams({ strict: false }) as { taskId: string };
  const id = Number(taskId);

  const { data: taskData, isLoading: loading } = useTaskList();
  const item = taskData?.items.find((t) => t.id === id) ?? null;

  const handleBack = useCallback(() => {
    useUIStore.getState().setMergeItem(null);
    void navigate({ to: '/captain' });
  }, [navigate]);

  const handleOpenTerminal = useCallback(
    (opts: { project: string; cwd: string; resumeSessionId?: string }) => {
      void navigate({
        to: '/terminal',
        search: { project: opts.project, cwd: opts.cwd, resume: opts.resumeSessionId },
      });
    },
    [navigate],
  );

  if (!item) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {loading ? 'Loading...' : 'Task not found'}
      </div>
    );
  }

  return (
    <div className="h-full px-8 py-4">
      <ErrorBoundary fallbackLabel="Task detail">
        <TaskDetailView
          item={item}
          onBack={handleBack}
          onMerge={() => useUIStore.getState().setMergeItem(item)}
          onOpenTerminal={handleOpenTerminal}
        />
      </ErrorBoundary>
    </div>
  );
}
