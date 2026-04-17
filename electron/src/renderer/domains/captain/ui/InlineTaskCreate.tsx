import React, { useImperativeHandle, useRef, useState } from 'react';
import { useDraft } from '#renderer/domains/captain/runtime/useDraft';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/runtime/useConfig';
import { useTaskCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
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
  const [title, setTitle, clearTitleDraft] = useDraft('mando:draft:inlineTask');
  const hasDraft = title !== '';
  const {
    project,
    setProject: handleProjectChange,
    resetDrafts,
    persistProject,
  } = useTaskFormPersistence({
    draftProjectKey: 'mando:draft:inlineTask:project',
    hasDraft,
  });
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const [noAutoMerge, setNoAutoMerge] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();

  useImperativeHandle(ref, () => ({
    focus: () => inputRef.current?.focus(),
  }));

  const resetForm = () => {
    clearTitleDraft();
    resetDrafts();
    setNoAutoMerge(false);
    removeImage();
  };

  const canSubmit =
    !!trimmedTitle && (!projectRequired || !!effectiveProject) && !createMut.isPending;

  const handleSubmit = () => {
    if (!canSubmit) return;
    persistProject(effectiveProject);
    createMut.mutate(
      {
        title: trimmedTitle,
        project: effectiveProject || undefined,
        noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
        images: image ? [image] : undefined,
      },
      { onSuccess: () => resetForm() },
    );
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.metaKey && event.key === 'Enter') {
      event.preventDefault();
      handleSubmit();
    }
  };

  const handlePaste = (event: React.ClipboardEvent) => {
    const file = extractImageFromClipboard(event);
    if (file) setImageFile(file);
  };

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
