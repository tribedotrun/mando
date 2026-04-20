import React from 'react';
import { Button } from '#renderer/global/ui/button';

interface AddTaskFormBodyProps {
  title: string;
  setTitle: (v: string) => void;
  bulk: boolean;
  image: File | null;
  preview: string | null;
  removeImage: () => void;
  submitError: string | null;
  textareaRows: number;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  handlePaste: (e: React.ClipboardEvent) => void;
}

export function AddTaskFormBody({
  title,
  setTitle,
  bulk,
  image,
  preview,
  removeImage,
  submitError,
  textareaRows,
  inputRef,
  handlePaste,
}: AddTaskFormBodyProps): React.ReactElement {
  return (
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
  );
}
