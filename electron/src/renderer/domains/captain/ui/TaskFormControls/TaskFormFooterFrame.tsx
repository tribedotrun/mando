import React from 'react';
import { TaskSubmitButton } from '#renderer/domains/captain/ui/TaskComposerControls';

interface TaskFormFooterFrameProps {
  projectRequired: boolean;
  effectiveProject: string;
  canSubmit: boolean;
  isPending: boolean;
  onSubmit: () => void;
  testIdPrefix?: string;
  children: React.ReactNode;
}

export function TaskFormFooterFrame({
  children,
  projectRequired,
  effectiveProject,
  canSubmit,
  isPending,
  onSubmit,
}: TaskFormFooterFrameProps): React.ReactElement {
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
