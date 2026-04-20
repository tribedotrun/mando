import React from 'react';
import { useAddTaskForm } from '#renderer/domains/captain/runtime/useAddTaskForm';
import { Button } from '#renderer/global/ui/button';
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
  const {
    title,
    setTitle,
    bulk,
    setBulk,
    image,
    preview,
    setImageFile,
    removeImage,
    submitError,
    noAutoMerge,
    setNoAutoMerge,
    inputRef,
    createPhase,
    projects,
    globalAutoMerge,
    effectiveProject,
    projectRequired,
    textareaRows,
    canSubmit,
    handleSubmit,
    handleKeyDown,
    handlePaste,
    handleProjectChange,
  } = useAddTaskForm({ onClose, initialProject });

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-overlay"
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={handleKeyDown}
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
              variant={bulk ? 'outline' : 'secondary'}
              size="xs"
              onClick={() => setBulk(!bulk)}
              className={bulk ? 'text-foreground' : ''}
            >
              Bulk
            </Button>
          </div>
        </div>

        <AddTaskFormBody
          title={title}
          setTitle={setTitle}
          bulk={bulk}
          image={image}
          preview={preview}
          removeImage={removeImage}
          submitError={submitError}
          textareaRows={textareaRows}
          inputRef={inputRef}
          handlePaste={handlePaste}
        />

        {bulk ? (
          <BulkTaskFormFooter
            projects={projects}
            effectiveProject={effectiveProject}
            onProjectChange={handleProjectChange}
            projectRequired={projectRequired}
            canSubmit={canSubmit}
            isPending={createPhase === 'active'}
            onSubmit={handleSubmit}
          />
        ) : (
          <TaskFormFooter
            projects={projects}
            effectiveProject={effectiveProject}
            onProjectChange={handleProjectChange}
            projectRequired={projectRequired}
            globalAutoMerge={globalAutoMerge}
            noAutoMerge={noAutoMerge}
            onNoAutoMergeChange={setNoAutoMerge}
            onImageSelect={setImageFile}
            canSubmit={canSubmit}
            isPending={createPhase === 'active'}
            onSubmit={handleSubmit}
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
