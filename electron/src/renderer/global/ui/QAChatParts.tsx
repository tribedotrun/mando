import React from 'react';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

interface ImageQAComposerProps {
  question: string;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  handleChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => void;
  doSubmit: () => void;
  pending: boolean;
  image: File | null;
  preview: string | null;
  setImageFile: (file: File) => void;
  removeImage: () => void;
  fileRef: React.RefObject<HTMLInputElement | null>;
  placeholder?: string;
  formClassName?: string;
  formStyle?: React.CSSProperties;
}

export function ImageQAComposer({
  question,
  textareaRef,
  handleChange,
  doSubmit,
  pending,
  image,
  preview,
  setImageFile,
  removeImage,
  fileRef,
  placeholder,
  formClassName = '',
  formStyle,
}: ImageQAComposerProps): React.ReactElement {
  return (
    <>
      {preview && image && (
        <div className={`flex items-center gap-1.5 ${formClassName}`}>
          <button
            type="button"
            onClick={removeImage}
            className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
          >
            <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
            <span className="max-w-[160px] truncate">{image.name}</span>
            <X size={10} className="shrink-0 opacity-60" />
          </button>
        </div>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          doSubmit();
        }}
        className={`flex items-end gap-2 ${formClassName}`}
        style={formStyle}
      >
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) setImageFile(file);
            event.target.value = '';
          }}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={() => fileRef.current?.click()}
          disabled={pending}
          aria-label="Attach image"
          className="shrink-0 text-muted-foreground"
        >
          <Paperclip size={14} />
        </Button>
        <Textarea
          ref={textareaRef}
          value={question}
          onChange={handleChange}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              doSubmit();
            }
          }}
          onPaste={(event) => {
            const file = extractImageFromClipboard(event);
            if (file) setImageFile(file);
          }}
          placeholder={placeholder}
          className="min-h-0 flex-1 resize-none overflow-y-auto border-0 bg-muted shadow-none [scrollbar-width:none] focus-visible:ring-0 dark:bg-muted"
          rows={1}
          autoFocus
        />
        <Button type="submit" size="sm" disabled={pending || !question.trim()}>
          Ask
        </Button>
      </form>
    </>
  );
}
