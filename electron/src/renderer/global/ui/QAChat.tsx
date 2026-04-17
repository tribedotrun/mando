export type { QAEntry } from '#renderer/global/ui/QAChatSurface';
import React, { useCallback, useRef } from 'react';
import Markdown from 'react-markdown';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { useQAComposer } from '#renderer/global/runtime/useQAComposer';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
import { QAChatSurface, type QAChatSurfaceProps } from '#renderer/global/ui/QAChatSurface';

function MarkdownAssistantMessage({ text }: { text: string }): React.ReactElement {
  return <Markdown>{text}</Markdown>;
}

interface SharedQAChatProps extends Omit<QAChatSurfaceProps, 'composer' | 'AssistantMessage'> {
  onAsk: (question: string, images?: File[]) => void;
  formClassName?: string;
  formStyle?: React.CSSProperties;
}

export function TextQAChat({
  onAsk,
  pending,
  placeholder,
  formClassName = '',
  formStyle,
  ...surfaceProps
}: SharedQAChatProps): React.ReactElement {
  const scrollRef = surfaceProps.scrollRef;
  const localScrollRef = useRef<(() => void) | null>(null);
  const composerScrollRef = scrollRef ?? localScrollRef;
  const scrollToBottom = useCallback(() => composerScrollRef.current?.(), [composerScrollRef]);
  const { question, textareaRef, handleChange, submit } = useQAComposer(
    onAsk,
    pending,
    scrollToBottom,
  );

  return (
    <QAChatSurface
      {...surfaceProps}
      pending={pending}
      placeholder={placeholder}
      scrollRef={composerScrollRef}
      composer={
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
          className={`flex items-end gap-2 ${formClassName}`}
          style={formStyle}
        >
          <Textarea
            ref={textareaRef}
            value={question}
            onChange={handleChange}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
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
      }
    />
  );
}

export function MarkdownImageQAChat({
  onAsk,
  pending,
  placeholder,
  formClassName = '',
  formStyle,
  ...surfaceProps
}: SharedQAChatProps): React.ReactElement {
  const scrollRef = surfaceProps.scrollRef;
  const localScrollRef = useRef<(() => void) | null>(null);
  const composerScrollRef = scrollRef ?? localScrollRef;
  const scrollToBottom = useCallback(() => composerScrollRef.current?.(), [composerScrollRef]);
  const { question, textareaRef, handleChange, submit } = useQAComposer(
    onAsk,
    pending,
    scrollToBottom,
  );
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const fileRef = useRef<HTMLInputElement>(null);

  return (
    <QAChatSurface
      {...surfaceProps}
      pending={pending}
      placeholder={placeholder}
      scrollRef={composerScrollRef}
      AssistantMessage={MarkdownAssistantMessage}
      composer={
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
              if (submit(image ? [image] : undefined)) removeImage();
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
                  if (submit(image ? [image] : undefined)) removeImage();
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
      }
    />
  );
}
