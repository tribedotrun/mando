import React from 'react';
import { useAddTaskForm } from '#renderer/domains/captain/runtime/useAddTaskForm';
import { Button } from '#renderer/global/ui/primitives/button';
import { BulkTaskFormFooter, TaskFormFooter } from '#renderer/domains/captain/ui/TaskFormControls';
import { AddTaskFormBody } from '#renderer/domains/captain/ui/AddTaskFormBody';

interface Props {
  open: boolean;
  onClose: () => void;
  initialProject?: string | null;
}

function AddTaskFormInner({
  onClose,
  initialProject,
}: {
  onClose: () => void;
  initialProject?: string | null;
}): React.ReactElement {
  const form = useAddTaskForm({ onClose, initialProject });
  const pending = form.submit.createPhase === 'active';

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-overlay"
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={form.events.handleKeyDown}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New task"
        className="flex max-h-[90vh] w-[640px] max-w-[92vw] flex-col overflow-hidden rounded-xl bg-card shadow-2xl"
      >
        <div className="px-5 pb-2 pt-5">
          <div className="flex items-center justify-between">
            <div className="text-heading text-foreground">New task</div>
            <Button
              variant={form.draft.bulk ? 'outline' : 'secondary'}
              size="xs"
              onClick={() => form.draft.setBulk(!form.draft.bulk)}
              className={form.draft.bulk ? 'text-foreground' : ''}
            >
              Bulk
            </Button>
          </div>
        </div>

        <AddTaskFormBody
          title={form.draft.title}
          setTitle={form.draft.setTitle}
          bulk={form.draft.bulk}
          image={form.image.image}
          preview={form.image.preview}
          removeImage={form.image.removeImage}
          submitError={form.draft.submitError}
          textareaRows={form.draft.textareaRows}
          inputRef={form.draft.inputRef}
          handlePaste={form.events.handlePaste}
        />

        {form.draft.bulk ? (
          <BulkTaskFormFooter
            projects={form.project.projects}
            effectiveProject={form.project.effectiveProject}
            onProjectChange={form.project.handleProjectChange}
            projectRequired={form.project.projectRequired}
            canSubmit={form.submit.canSubmit}
            isPending={pending}
            onSubmit={form.submit.handleSubmit}
          />
        ) : (
          <TaskFormFooter
            projects={form.project.projects}
            effectiveProject={form.project.effectiveProject}
            onProjectChange={form.project.handleProjectChange}
            projectRequired={form.project.projectRequired}
            globalAutoMerge={form.autoMerge.globalAutoMerge}
            noAutoMerge={form.autoMerge.noAutoMerge}
            onNoAutoMergeChange={form.autoMerge.setNoAutoMerge}
            planning={form.planMode.planning}
            onPlanningChange={form.planMode.setPlanning}
            onImageSelect={form.image.setImageFile}
            canSubmit={form.submit.canSubmit}
            isPending={pending}
            onSubmit={form.submit.handleSubmit}
          />
        )}
      </div>
    </div>
  );
}

export function CreateTaskModal({
  open,
  onClose,
  initialProject,
}: Props): React.ReactElement | null {
  if (!open) return null;
  return <AddTaskFormInner onClose={onClose} initialProject={initialProject} />;
}
