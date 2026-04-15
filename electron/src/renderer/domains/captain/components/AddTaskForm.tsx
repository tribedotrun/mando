import React, { useRef, useState } from 'react';
import { ArrowUp, Paperclip } from 'lucide-react';
import { useDraft } from '#renderer/global/hooks/useDraft';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useProjects } from '#renderer/domains/settings';
import { useConfig } from '#renderer/hooks/queries';
import { useTaskCreate, useTaskBulkCreate } from '#renderer/hooks/mutations';
import { bulkTextareaRows, shortRepo } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Switch } from '#renderer/components/ui/switch';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/components/ui/tooltip';
import { Combobox } from '#renderer/components/ui/combobox';

const AUTOFOCUS_DELAY_MS = 50;

const LAST_PROJECT_KEY = 'mando:lastProject';
const DRAFT_BULK_KEY = 'mando:draft:newTask:bulk';
const DRAFT_PROJECT_KEY = 'mando:draft:newTask:project';

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
  const [bulk, setBulk] = useState(() => hasDraft && localStorage.getItem(DRAFT_BULK_KEY) === '1');
  // The health API returns project display names (not paths), and the sidebar
  // filter is also a display name -- so initialProject can be used directly.
  // When restoring a draft, the draft-specific project takes precedence.
  const [project, setProject] = useState(() => {
    if (hasDraft) {
      const draftProject = localStorage.getItem(DRAFT_PROJECT_KEY);
      if (draftProject !== null) return draftProject;
    }
    return initialProject ?? localStorage.getItem(LAST_PROJECT_KEY) ?? '';
  });
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [noAutoMerge, setNoAutoMerge] = useState(false);

  const titleRef = useRef(title);
  titleRef.current = title;
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const createMut = useTaskCreate();
  const bulkCreateMut = useTaskBulkCreate();
  const createPhase = createMut.isPending || bulkCreateMut.isPending ? 'active' : 'idle';

  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const savedProject = project && projects.includes(project) ? project : '';
  const effectiveProject = savedProject || (projects.length === 1 ? projects[0] : '');
  const projectRequired = projects.length !== 1;
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 5;

  useMountEffect(() => {
    setTimeout(() => inputRef.current?.focus(), AUTOFOCUS_DELAY_MS);
    return () => {
      if (!titleRef.current.trim()) {
        localStorage.removeItem(DRAFT_BULK_KEY);
        localStorage.removeItem(DRAFT_PROJECT_KEY);
      }
    };
  });

  const previewRef = useRef(preview);
  previewRef.current = preview;
  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  const resetForm = () => {
    setBulk(false);
    clearTitleDraft();
    localStorage.removeItem(DRAFT_BULK_KEY);
    localStorage.removeItem(DRAFT_PROJECT_KEY);
    setSubmitError(null);
    setNoAutoMerge(false);
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
      localStorage.setItem(LAST_PROJECT_KEY, value);
      localStorage.setItem(DRAFT_PROJECT_KEY, value);
    } else {
      localStorage.removeItem(LAST_PROJECT_KEY);
      localStorage.removeItem(DRAFT_PROJECT_KEY);
    }
  };

  const canSubmit =
    !!trimmedTitle && (!projectRequired || !!effectiveProject) && createPhase === 'idle';

  const handleSubmit = () => {
    if (!trimmedTitle) return;
    if (projectRequired && !effectiveProject) {
      setSubmitError('Select a project before handing work to Mando.');
      return;
    }

    if (effectiveProject) localStorage.setItem(LAST_PROJECT_KEY, effectiveProject);

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
    for (const item of e.clipboardData.items) {
      if (!item.type.startsWith('image/')) continue;
      e.preventDefault();
      const file = item.getAsFile();
      if (file) setImageFile(file);
      return;
    }
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
              onClick={() =>
                setBulk((b) => {
                  const next = !b;
                  if (next) localStorage.setItem(DRAFT_BULK_KEY, '1');
                  else localStorage.removeItem(DRAFT_BULK_KEY);
                  return next;
                })
              }
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

        <div className="flex shrink-0 items-center justify-between gap-4 px-5 py-3">
          <div className="flex min-w-0 items-center gap-2">
            {projects.length > 0 && (
              <Combobox
                data-testid="task-project-select"
                value={effectiveProject}
                onValueChange={handleProjectChange}
                options={[
                  ...projects.map((item) => ({
                    value: item,
                    label: shortRepo(item),
                  })),
                ]}
                placeholder="Project..."
                searchPlaceholder="Search projects..."
                emptyText="No projects found."
              />
            )}

            {!bulk && (
              <>
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
                  className="text-muted-foreground"
                >
                  <Paperclip size={16} />
                </Button>
              </>
            )}

            {!bulk && globalAutoMerge && (
              <label className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
                <Switch
                  checked={noAutoMerge}
                  onCheckedChange={setNoAutoMerge}
                  className="scale-75"
                />
                Skip auto-merge
              </label>
            )}

            {projectRequired && !effectiveProject && (
              <span className="text-[12px] text-stale">Choose a project.</span>
            )}
          </div>

          <div className="flex items-center gap-3">
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    data-testid="submit-task-btn"
                    onClick={handleSubmit}
                    disabled={!canSubmit}
                    variant="default"
                    size="icon-xs"
                    aria-label="Create task"
                    className="shrink-0 rounded-full transition-colors"
                  >
                    {createPhase === 'active' ? (
                      <svg
                        className="animate-spin"
                        width="14"
                        height="14"
                        viewBox="0 0 14 14"
                        fill="none"
                      >
                        <circle
                          cx="7"
                          cy="7"
                          r="5.5"
                          stroke="currentColor"
                          strokeWidth="2"
                          opacity="0.3"
                        />
                        <path
                          d="M12.5 7a5.5 5.5 0 0 0-5.5-5.5"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                        />
                      </svg>
                    ) : (
                      <ArrowUp size={14} strokeWidth={2} />
                    )}
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top" className="text-xs">
                  Create ⌘↵
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        </div>
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
