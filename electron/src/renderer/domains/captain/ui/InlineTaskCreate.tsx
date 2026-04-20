import React, { useImperativeHandle } from 'react';
import { useInlineTaskCreate } from '#renderer/domains/captain/runtime/useInlineTaskCreate';
import { Button } from '#renderer/global/ui/button';
import {
  TaskAttachmentButton,
  TaskAutoMergeToggle,
  TaskProjectSelect,
  TaskSubmitButton,
} from '#renderer/domains/captain/ui/TaskComposerControls';

export interface InlineTaskCreateHandle {
  focus: () => void;
}

interface InlineTaskCreateProps {
  ref?: React.Ref<InlineTaskCreateHandle>;
}

export function InlineTaskCreate({ ref }: InlineTaskCreateProps): React.ReactElement {
  const {
    title,
    setTitle,
    image,
    preview,
    setImageFile,
    removeImage,
    noAutoMerge,
    setNoAutoMerge,
    inputRef,
    createMut,
    projects,
    globalAutoMerge,
    effectiveProject,
    projectRequired,
    canSubmit,
    handleSubmit,
    handleKeyDown,
    handlePaste,
    handleProjectChange,
  } = useInlineTaskCreate();

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current?.focus(),
  }));

  return (
    <div className="mx-auto w-full max-w-[640px]">
      <div className="rounded-xl bg-muted">
        <textarea
          ref={inputRef}
          data-testid="inline-task-input"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder="What needs to be done?"
          rows={3}
          className="w-full resize-none rounded-xl bg-transparent px-4 pb-2 pt-4 text-sm text-foreground placeholder:text-text-3 focus:outline-none"
          style={{ caretColor: 'var(--foreground)' }}
        />

        {preview && image && (
          <div className="flex items-center gap-3 px-4 pb-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-secondary">
              <img
                src={preview}
                alt={image.name}
                className="max-h-10 max-w-10 rounded-md object-contain"
              />
            </div>
            <span className="min-w-0 truncate text-caption text-muted-foreground">
              {image.name}
            </span>
            <Button variant="ghost" size="xs" onClick={removeImage} className="text-text-3">
              Remove
            </Button>
          </div>
        )}

        <div className="flex items-center gap-2 px-3 pb-3">
          <TaskProjectSelect
            projects={projects}
            value={effectiveProject}
            onValueChange={handleProjectChange}
            testId="inline-task-project"
          />
          <TaskAttachmentButton
            onImageSelect={setImageFile}
            size="icon-sm"
            className="text-text-3"
          />
          {globalAutoMerge && (
            <TaskAutoMergeToggle
              checked={noAutoMerge}
              onCheckedChange={setNoAutoMerge}
              className="flex items-center gap-1.5 text-caption text-text-3"
            />
          )}

          <span className="flex-1" />

          {projectRequired && !effectiveProject && (
            <span className="text-caption text-text-3">Choose a project</span>
          )}

          <TaskSubmitButton
            testId="inline-task-submit"
            disabled={!canSubmit}
            pending={createMut.isPending}
            onSubmit={handleSubmit}
          />
        </div>
      </div>
    </div>
  );
}
