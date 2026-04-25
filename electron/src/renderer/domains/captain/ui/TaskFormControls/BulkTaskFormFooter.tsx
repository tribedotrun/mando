import React from 'react';
import { TaskProjectSelect } from '#renderer/domains/captain/ui/TaskComposerControls';
import { TaskFormFooterFrame } from '#renderer/domains/captain/ui/TaskFormControls/TaskFormFooterFrame';

interface BulkTaskFormFooterProps {
  projects: string[];
  effectiveProject: string;
  onProjectChange: (value: string) => void;
  projectRequired: boolean;
  canSubmit: boolean;
  isPending: boolean;
  onSubmit: () => void;
  testIdPrefix?: string;
}

export function BulkTaskFormFooter({
  projects,
  effectiveProject,
  onProjectChange,
  projectRequired,
  canSubmit,
  isPending,
  onSubmit,
  testIdPrefix = 'task',
}: BulkTaskFormFooterProps): React.ReactElement {
  return (
    <TaskFormFooterFrame
      projectRequired={projectRequired}
      effectiveProject={effectiveProject}
      canSubmit={canSubmit}
      isPending={isPending}
      onSubmit={onSubmit}
      testIdPrefix={testIdPrefix}
    >
      <TaskProjectSelect
        projects={projects}
        value={effectiveProject}
        onValueChange={onProjectChange}
        testId={`${testIdPrefix}-project-select`}
      />
    </TaskFormFooterFrame>
  );
}
