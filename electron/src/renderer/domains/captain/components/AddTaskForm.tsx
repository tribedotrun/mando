import React, { useRef, useState } from 'react';
import { inputStyle, inputCls } from '#renderer/styles';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useProjects } from '#renderer/domains/settings';
import { useBulkCreateStore } from '#renderer/domains/captain/stores/bulkCreateStore';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { bulkTextareaRows, getErrorMessage, shortRepo } from '#renderer/utils';

const LAST_PROJECT_KEY = 'mando:lastProject';
const titleInputCls = `${inputCls} resize-none`;
const projectSelectCls = 'rounded-md px-3 py-2 text-label';
const footerButtonCls =
  'px-4 py-2 text-[13px] font-semibold transition-colors hover:bg-[var(--color-accent-hover)] active:bg-[var(--color-accent-pressed)] disabled:opacity-40';
const bulkToggleCls = 'rounded-md px-2 py-0.5 text-label transition-colors';

interface Props {
  open: boolean;
  onClose: () => void;
}

function AddTaskFormInner({ onClose }: { onClose: () => void }): React.ReactElement {
  const [bulk, setBulk] = useState(false);
  const [title, setTitle] = useState('');
  const [project, setProject] = useState(() => localStorage.getItem(LAST_PROJECT_KEY) ?? '');
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const add = useTaskStore((s) => s.add);
  const startBulk = useBulkCreateStore((s) => s.start);

  const projects = useProjects();

  const savedProject = project && projects.includes(project) ? project : '';
  const effectiveProject = savedProject || (projects.length === 1 ? projects[0] : '');
  const projectRequired = projects.length > 1;
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 5;

  useMountEffect(() => {
    setTimeout(() => inputRef.current?.focus(), 50);
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
    setTitle('');
    setSubmitError(null);
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
    setProject(value);
    if (value) localStorage.setItem(LAST_PROJECT_KEY, value);
    else localStorage.removeItem(LAST_PROJECT_KEY);
  };

  const canSubmit = !submitting && !!trimmedTitle && (!projectRequired || !!effectiveProject);

  const handleSubmit = async () => {
    if (!trimmedTitle) return;
    if (projectRequired && !effectiveProject) {
      setSubmitError('Select a project before handing work to Mando.');
      return;
    }

    if (effectiveProject) localStorage.setItem(LAST_PROJECT_KEY, effectiveProject);

    if (bulk) {
      startBulk(trimmedTitle, effectiveProject || undefined);
      resetForm();
      onClose();
      return;
    }

    setSubmitting(true);
    setSubmitError(null);
    try {
      await add({
        title: trimmedTitle,
        project: effectiveProject || undefined,
        images: image ? [image] : undefined,
      });

      resetForm();
      onClose();
    } catch (err) {
      setSubmitError(getErrorMessage(err, 'Failed to create task'));
    } finally {
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey && e.key === 'Enter') {
      e.preventDefault();
      void handleSubmit();
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
      className="fixed inset-0 z-[200] flex items-center justify-center"
      style={{ background: 'var(--color-overlay)' }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={handleKeyDown}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New task"
        className="flex max-h-[90vh] w-[640px] max-w-[92vw] flex-col overflow-hidden"
        style={{
          background: 'var(--color-surface-1)',
          border: '1px solid var(--color-border-subtle)',
          borderRadius: 'var(--radius-hero)',
          boxShadow: '0 24px 64px #00000099, 0 4px 16px #00000066',
        }}
      >
        <div className="px-5 pb-2 pt-5">
          <div className="flex items-center justify-between">
            <div className="text-heading" style={{ color: 'var(--color-text-1)' }}>
              New task
            </div>
            <button
              type="button"
              onClick={() => setBulk((b) => !b)}
              className={bulkToggleCls}
              style={{
                background: bulk ? 'var(--color-accent-wash)' : 'var(--color-surface-2)',
                color: bulk ? 'var(--color-accent)' : 'var(--color-text-3)',
                border: `1px solid ${bulk ? 'var(--color-accent)' : 'var(--color-border-subtle)'}`,
              }}
            >
              Bulk
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4">
          <div className="space-y-4">
            {submitError && (
              <div
                className="rounded-lg px-3 py-2 text-[13px]"
                style={{
                  background: 'color-mix(in srgb, var(--color-error) 16%, transparent)',
                  border: '1px solid color-mix(in srgb, var(--color-error) 35%, transparent)',
                  color: 'var(--color-text-1)',
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
                className={titleInputCls}
                style={{ ...inputStyle, caretColor: 'var(--color-accent)' }}
              />
            </div>

            {!bulk && preview && image && (
              <div
                className="rounded-xl border p-3"
                style={{ borderColor: 'var(--color-border-subtle)' }}
              >
                <div className="mb-2 text-label" style={{ color: 'var(--color-text-4)' }}>
                  Reference image
                </div>
                <div className="flex items-start gap-3">
                  <div
                    className="flex h-20 w-20 shrink-0 items-center justify-center rounded-md"
                    style={{
                      border: '1px solid var(--color-border)',
                      background: 'var(--color-surface-2)',
                    }}
                  >
                    <img
                      src={preview}
                      alt={image.name}
                      className="max-h-20 max-w-20 rounded-md object-contain"
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px]" style={{ color: 'var(--color-text-2)' }}>
                      {image.name}
                    </div>
                    <button
                      type="button"
                      onClick={removeImage}
                      className="mt-2 rounded-md px-2.5 py-1 text-[12px]"
                      style={{
                        color: 'var(--color-text-2)',
                        border: '1px solid var(--color-border)',
                      }}
                    >
                      Remove image
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        <div
          className="flex shrink-0 items-center justify-between gap-4 border-t px-5 py-3"
          style={{ borderColor: 'var(--color-border-subtle)' }}
        >
          <div className="flex min-w-0 items-center gap-2">
            {projects.length > 0 && (
              <select
                data-testid="task-project-select"
                value={effectiveProject}
                onChange={(e) => handleProjectChange(e.target.value)}
                aria-label="Select project"
                className={projectSelectCls}
                style={{
                  background: 'var(--color-surface-2)',
                  border: '1px solid var(--color-border-subtle)',
                  color: 'var(--color-text-2)',
                }}
              >
                {projects.length > 1 && <option value="">Project…</option>}
                {projects.map((item) => (
                  <option key={item} value={item}>
                    {shortRepo(item).toUpperCase()}
                  </option>
                ))}
              </select>
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
                <button
                  type="button"
                  onClick={() => fileRef.current?.click()}
                  className="flex h-9 w-9 items-center justify-center rounded-md transition-colors hover:bg-[var(--color-surface-3)]"
                  style={{ color: 'var(--color-text-3)' }}
                  aria-label="Attach image"
                  title="Attach image (or paste)"
                >
                  <svg
                    width="15"
                    height="15"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" />
                  </svg>
                </button>
              </>
            )}

            {projectRequired && !effectiveProject && (
              <span className="text-[12px]" style={{ color: 'var(--color-stale)' }}>
                Choose a project.
              </span>
            )}
          </div>

          <div className="flex items-center gap-3">
            <button
              data-testid="submit-task-btn"
              onClick={() => void handleSubmit()}
              disabled={!canSubmit}
              className={footerButtonCls}
              style={{
                color: 'var(--color-bg)',
                borderRadius: 'var(--radius-button)',
                background: 'var(--color-accent)',
              }}
            >
              {submitting ? 'Working…' : 'Create'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function CreateTaskModal({ open, onClose }: Props): React.ReactElement | null {
  if (!open) return null;
  return <AddTaskFormInner onClose={onClose} />;
}
