import React from 'react';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
import { useImageQAComposer } from '#renderer/domains/scout/runtime/useImageQAComposer';

interface ImageQAComposerProps {
  onAsk: (question: string, images?: File[]) => void;
  pending: boolean;
  scrollRef: React.MutableRefObject<(() => void) | null>;
  draftKey: string;
  placeholder?: string;
  formClassName?: string;
  formStyle?: React.CSSProperties;
}

export function ImageQAComposer({
  onAsk,
  pending,
  scrollRef,
  draftKey,
  placeholder,
  formClassName = '',
  formStyle,
}: ImageQAComposerProps): React.ReactElement {
  const composer = useImageQAComposer(onAsk, pending, scrollRef, draftKey);

  return (
    <>
      {composer.image.preview && composer.image.image && (
        <div className={`flex items-center gap-1.5 ${formClassName}`}>
          <button
            type="button"
            onClick={composer.image.removeImage}
            className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
          >
            <img src={composer.image.preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
            <span className="max-w-[160px] truncate">{composer.image.image.name}</span>
            <X size={10} className="shrink-0 opacity-60" />
          </button>
        </div>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          composer.submit.doSubmit();
        }}
        className={`flex items-end gap-2 ${formClassName}`}
        style={formStyle}
      >
        <input
          ref={composer.image.fileRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) composer.image.setImageFile(file);
            event.target.value = '';
          }}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={() => composer.image.fileRef.current?.click()}
          disabled={pending}
          aria-label="Attach image"
          className="shrink-0 text-muted-foreground"
        >
          <Paperclip size={14} />
        </Button>
        <Textarea
          ref={composer.text.textareaRef}
          value={composer.text.question}
          onChange={composer.text.handleChange}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              composer.submit.doSubmit();
            }
          }}
          onPaste={(event) => {
            const file = extractImageFromClipboard(event);
            if (file) composer.image.setImageFile(file);
          }}
          placeholder={placeholder}
          className="min-h-0 flex-1 resize-none overflow-y-auto border-0 bg-muted shadow-none [scrollbar-width:none] focus-visible:ring-0 dark:bg-muted"
          rows={1}
          autoFocus
        />
        <Button type="submit" size="sm" disabled={pending || !composer.text.question.trim()}>
          Ask
        </Button>
      </form>
    </>
  );
}
