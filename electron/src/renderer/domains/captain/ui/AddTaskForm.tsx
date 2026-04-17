import React, { useRef, useState } from 'react';
import { useDraft } from '#renderer/domains/captain/runtime/useDraft';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/runtime/useConfig';
import { useTaskCreate, useTaskBulkCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { bulkTextareaRows } from '#renderer/global/service/utils';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
import { Button } from '#renderer/global/ui/button';
import { BulkTaskFormFooter, TaskFormFooter } from '#renderer/domains/captain/ui/TaskFormControls';

const AUTOFOCUS_DELAY_MS = 50;

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
  const [title, setTitle, clearTitleDraft] = useDraft('mando:draft:newTask');
  const hasDraft = title !== '';
  const {
    bulk,
    setBulk,
    project,
    setProject: handleProjectChange,
    resetDrafts,
    cleanupIfEmpty,
    persistProject,
  } = useTaskFormPersistence({
    draftProjectKey: 'mando:draft:newTask:project',
    draftBulkKey: 'mando:draft:newTask:bulk',
    hasDraft,
    initialProject,
  });
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [noAutoMerge, setNoAutoMerge] = useState(false);

  const titleRef = useRef(title);
  titleRef.current = title;
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const bulkCreateMut = useTaskBulkCreate();
  const createPhase = createMut.isPending || bulkCreateMut.isPending ? 'active' : 'idle';

  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 5;

  useMountEffect(() => {
    setTimeout(() => inputRef.current?.focus(), AUTOFOCUS_DELAY_MS);
    return () => cleanupIfEmpty(!titleRef.current.trim());
  });

  const resetForm = () => {
    setBulk(false);
    clearTitleDraft();
    resetDrafts();
    setSubmitError(null);
    setNoAutoMerge(false);
    removeImage();
  };

  const canSubmit =
    !!trimmedTitle && (!projectRequired || !!effectiveProject) && createPhase === 'idle';

  const handleSubmit = () => {
    if (!trimmedTitle) return;
    if (projectRequired && !effectiveProject) {
      setSubmitError('Select a project before handing work to Mando.');
      return;
    }
    persistProject(effectiveProject);
    if (bulk) {
      bulkCreateMut.mutate({ text: trimmedTitle, project: effectiveProject });
    } else {
      createMut.mutate({
        title: trimmedTitle,
        project: effectiveProject || undefined,
        noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
        images: image ? [image] : undefined,
      });
    }
    resetForm();
    onClose();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey && e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === 'Escape') onClose();
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    const file = extractImageFromClipboard(e);
    if (file) setImageFile(file);
  };

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

        <div className="flex-1 overflow-y-auto px-5 py-4">
          <div className="space-y-4">
            {submitError && (
              <div
                className="rounded-lg px-3 py-2 text-[13px] text-foreground"
                style={{
                  background: 'color-mix(in srgb, var(--destructive) 16%, transparent)',
                }}
              >
                {submitError}
              </div>
            )}

            <div>
              <textarea
                ref={inputRef}
                data-testid="task-title-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                onPaste={bulk ? undefined : handlePaste}
                placeholder={
                  bulk
                    ? 'Describe your tasks, one per line, or free-form.\nAI will parse individual items.'
                    : 'What needs to be done?'
                }
                rows={textareaRows}
                className="w-full resize-none rounded-md bg-muted px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
                style={{ caretColor: 'var(--foreground)' }}
              />
            </div>

            {!bulk && preview && image && (
              <div className="rounded-xl bg-muted p-3">
                <div className="mb-2 text-label text-text-4">Reference image</div>
                <div className="flex items-start gap-3">
                  <div className="flex h-20 w-20 shrink-0 items-center justify-center rounded-md bg-secondary">
                    <img
                      src={preview}
                      alt={image.name}
                      className="max-h-20 max-w-20 rounded-md object-contain"
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] text-muted-foreground">{image.name}</div>
                    <Button variant="outline" size="xs" className="mt-2" onClick={removeImage}>
                      Remove image
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

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
