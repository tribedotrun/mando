import React from 'react';
import { ClipboardCheck } from 'lucide-react';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useAddProjectFromPicker } from '#renderer/global/runtime/useAddProjectFromPicker';
import { Button } from '#renderer/global/ui/button';
import { EmptyState } from '#renderer/global/ui/EmptyState';

export function TaskEmptyState(): React.ReactElement {
  const projects = useProjects();
  const hasProjects = projects.length > 0;
  const { pickAndAdd: handleAddProject, adding } = useAddProjectFromPicker();

  return (
    <EmptyState
      icon={<ClipboardCheck size={48} color="var(--text-4)" strokeWidth={1} />}
      heading={hasProjects ? 'No tasks yet' : 'Add a project to get started'}
      description={
        hasProjects
          ? 'Create a task and Captain will pick it up automatically.'
          : 'Mando needs a project folder to manage tasks.'
      }
    >
      {!hasProjects && (
        <Button onClick={() => void handleAddProject()} disabled={adding}>
          {adding ? 'Adding...' : 'Add project'}
        </Button>
      )}
    </EmptyState>
  );
}
