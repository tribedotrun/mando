import React, { useCallback } from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useUIStore } from '#renderer/app/uiStore';
import { CaptainView } from '#renderer/domains/captain/components/CaptainView';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';

export function CaptainPage(): React.ReactElement {
  const navigate = useNavigate();
  const project = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project,
  });

  const handleOpenDetail = useCallback(
    (item: { id: number }) => {
      void navigate({ to: '/captain/tasks/$taskId', params: { taskId: String(item.id) } });
    },
    [navigate],
  );

  const handleCreateTask = useCallback(() => {
    useUIStore.getState().openCreateTask();
  }, []);

  return (
    <div className="absolute inset-0 overflow-auto bg-background px-4 pb-2">
      <ErrorBoundary fallbackLabel="Captain view">
        <CaptainView
          projectFilter={project ?? null}
          onCreateTask={handleCreateTask}
          onOpenDetail={handleOpenDetail}
          active
        />
      </ErrorBoundary>
    </div>
  );
}
