import React from 'react';
import {
  TaskAttachmentButton,
  TaskAutoMergeToggle,
  TaskProjectSelect,
  TaskSubmitButton,
} from '#renderer/domains/captain/ui/TaskComposerControls';

interface SharedTaskFormFooterProps {
  projects: string[];
  effectiveProject: string;
  onProjectChange: (value: string) => void;
  projectRequired: boolean;
  canSubmit: boolean;
  isPending: boolean;
  onSubmit: () => void;
  testIdPrefix?: string;
}

interface StandardTaskFormFooterProps extends SharedTaskFormFooterProps {
  globalAutoMerge: boolean;
  noAutoMerge: boolean;
  onNoAutoMergeChange: (value: boolean) => void;
  onImageSelect: (file: File) => void;
}

function TaskFormFooterFrame({
  children,
  projectRequired,
  effectiveProject,
  canSubmit,
  isPending,
  onSubmit,
}: React.PropsWithChildren<
  Pick<
    SharedTaskFormFooterProps,
    'projectRequired' | 'effectiveProject' | 'canSubmit' | 'isPending' | 'onSubmit' | 'testIdPrefix'
  >
>): React.ReactElement {
  return (
    <div className="flex shrink-0 items-center justify-between gap-4 px-5 py-3">
      <div className="flex min-w-0 items-center gap-2">
        {children}
        {projectRequired && !effectiveProject && (
          <span className="text-[12px] text-stale">Choose a project.</span>
        )}
      </div>

      <div className="flex items-center gap-3">
        <TaskSubmitButton
          testId="submit-task-btn"
          disabled={!canSubmit}
          pending={isPending}
          onSubmit={onSubmit}
        />
      </div>
    </div>
  );
}

export function TaskFormFooter({
  projects,
  effectiveProject,
  onProjectChange,
  projectRequired,
  globalAutoMerge,
  noAutoMerge,
  onNoAutoMergeChange,
  onImageSelect,
  canSubmit,
  isPending,
  onSubmit,
  testIdPrefix = 'task',
}: StandardTaskFormFooterProps): React.ReactElement {
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
    </TaskFormFooterFrame>
  );
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
}: SharedTaskFormFooterProps): React.ReactElement {
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
