import React, { useRef, useState, useImperativeHandle, forwardRef } from 'react';
import { Paperclip } from 'lucide-react';
import { useDraft } from '#renderer/global/hooks/useDraft';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useProjects } from '#renderer/domains/settings';
import { useTaskCreate } from '#renderer/hooks/mutations';
import { shortRepo } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Kbd } from '#renderer/components/ui/kbd';
import { Combobox } from '#renderer/components/ui/combobox';

const LAST_PROJECT_KEY = 'mando:lastProject';
const DRAFT_PROJECT_KEY = 'mando:draft:inlineTask:project';

export interface InlineTaskCreateHandle {
  focus: () => void;
}

export const InlineTaskCreate = forwardRef<InlineTaskCreateHandle>(
  function InlineTaskCreate(_props, ref) {
    const [title, setTitle, clearTitleDraft] = useDraft('mando:draft:inlineTask');
    const hasDraft = title !== '';
    const [project, setProject] = useState(() => {
      if (hasDraft) {
        const draftProject = localStorage.getItem(DRAFT_PROJECT_KEY);
        if (draftProject !== null) return draftProject;
      }
      return localStorage.getItem(LAST_PROJECT_KEY) ?? '';
    });
    const [image, setImage] = useState<File | null>(null);
    const [preview, setPreview] = useState<string | null>(null);

    const inputRef = useRef<HTMLTextAreaElement>(null);
    const fileRef = useRef<HTMLInputElement>(null);
    const createMut = useTaskCreate();
    const projects = useProjects();

    const savedProject = project && projects.includes(project) ? project : '';
    const effectiveProject = savedProject || (projects.length === 1 ? projects[0] : '');
    const projectRequired = projects.length > 1;
    const trimmedTitle = title.trim();

    useImperativeHandle(ref, () => ({
      focus: () => inputRef.current?.focus(),
    }));

    const previewRef = useRef(preview);
    previewRef.current = preview;
    useMountEffect(() => {
      return () => {
        if (previewRef.current) URL.revokeObjectURL(previewRef.current);
      };
    });

    const resetForm = () => {
      clearTitleDraft();
      localStorage.removeItem(DRAFT_PROJECT_KEY);
      if (preview) URL.revokeObjectURL(preview);
      setImage(null);
      setPreview(null);
    };

    const setImageFile = (file: File) => {
      if (preview) URL.revokeObjectURL(preview);
      setImage(file);
      setPreview(URL.createObjectURL(file));
    };

    const removeImage = () => {
      if (preview) URL.revokeObjectURL(preview);
      setImage(null);
      setPreview(null);
    };

    const handleProjectChange = (value: string) => {
      const resolved = value === '__all__' ? '' : value;
      setProject(resolved);
      if (resolved) {
        localStorage.setItem(LAST_PROJECT_KEY, resolved);
        localStorage.setItem(DRAFT_PROJECT_KEY, resolved);
      } else {
        localStorage.removeItem(DRAFT_PROJECT_KEY);
      }
    };

    const canSubmit =
      !!trimmedTitle && (!projectRequired || !!effectiveProject) && !createMut.isPending;

    const handleSubmit = () => {
      if (!canSubmit) return;
      if (effectiveProject) localStorage.setItem(LAST_PROJECT_KEY, effectiveProject);
      createMut.mutate(
        {
          title: trimmedTitle,
          project: effectiveProject || undefined,
          images: image ? [image] : undefined,
        },
        { onSuccess: () => resetForm() },
      );
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
      if (e.metaKey && e.key === 'Enter') {
        e.preventDefault();
        handleSubmit();
      }
    };

    const handlePaste = (e: React.ClipboardEvent) => {
      for (const item of e.clipboardData.items) {
        if (!item.type.startsWith('image/')) continue;
        e.preventDefault();
        const file = item.getAsFile();
        if (file) setImageFile(file);
        return;
      }
    };

    return (
      <div className="mx-auto w-full max-w-[640px]">
        {/* Textarea */}
        <div className="rounded-xl bg-muted">
          <textarea
            ref={inputRef}
            data-testid="inline-task-input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder="What needs to be done?"
            rows={3}
            className="w-full resize-none rounded-xl bg-transparent px-4 pb-2 pt-4 text-sm text-foreground placeholder:text-text-3 focus:outline-none"
            style={{ caretColor: 'var(--foreground)' }}
          />

          {/* Image preview inside the textarea container */}
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

          {/* Controls bar inside the textarea container */}
          <div className="flex items-center gap-2 px-3 pb-3">
            {projects.length > 0 && (
              <Combobox
                data-testid="inline-task-project"
                value={effectiveProject}
                onValueChange={handleProjectChange}
                options={projects.map((item) => ({
                  value: item,
                  label: shortRepo(item),
                }))}
                placeholder="Project..."
                searchPlaceholder="Search projects..."
                emptyText="No projects found."
              />
            )}

            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) setImageFile(file);
                e.target.value = '';
              }}
            />
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => fileRef.current?.click()}
              aria-label="Attach image"
              className="text-text-3"
            >
              <Paperclip size={14} />
            </Button>

            <span className="flex-1" />

            {projectRequired && !effectiveProject && (
              <span className="text-caption text-text-3">Choose a project</span>
            )}

            <Button data-testid="inline-task-submit" onClick={handleSubmit} disabled={!canSubmit}>
              {createMut.isPending ? 'Creating...' : 'Create'}
              <Kbd className="bg-primary-foreground/20 text-primary-foreground">
                &#x2318;&#x21B5;
              </Kbd>
            </Button>
          </div>
        </div>
      </div>
    );
  },
);
