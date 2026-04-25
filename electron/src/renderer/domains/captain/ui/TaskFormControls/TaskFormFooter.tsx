import React from 'react';
import { TaskAttachmentButton } from '#renderer/domains/captain/ui/TaskAttachmentButton';
import {
  TaskAutoMergeToggle,
  TaskPlanModeToggle,
  TaskProjectSelect,
} from '#renderer/domains/captain/ui/TaskComposerControls';
import { TaskFormFooterFrame } from '#renderer/domains/captain/ui/TaskFormControls/TaskFormFooterFrame';

interface TaskFormFooterProps {
  projects: string[];
  effectiveProject: string;
  onProjectChange: (value: string) => void;
  projectRequired: boolean;
  globalAutoMerge: boolean;
  noAutoMerge: boolean;
  onNoAutoMergeChange: (value: boolean) => void;
  planning: boolean;
  onPlanningChange: (value: boolean) => void;
  onImageSelect: (file: File) => void;
  canSubmit: boolean;
  isPending: boolean;
  onSubmit: () => void;
  testIdPrefix?: string;
}

export function TaskFormFooter({
  projects,
  effectiveProject,
  onProjectChange,
  projectRequired,
  globalAutoMerge,
  noAutoMerge,
  onNoAutoMergeChange,
  planning,
  onPlanningChange,
  onImageSelect,
  canSubmit,
  isPending,
  onSubmit,
  testIdPrefix = 'task',
}: TaskFormFooterProps): React.ReactElement {
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
      <TaskAttachmentButton onImageSelect={onImageSelect} className="text-muted-foreground" />
      {globalAutoMerge && (
        <TaskAutoMergeToggle checked={noAutoMerge} onCheckedChange={onNoAutoMergeChange} />
      )}
      <TaskPlanModeToggle checked={planning} onCheckedChange={onPlanningChange} />
    </TaskFormFooterFrame>
  );
}
