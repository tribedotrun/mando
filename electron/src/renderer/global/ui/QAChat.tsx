export type { QAEntry } from '#renderer/global/ui/QAChatSurface';
import React, { useCallback, useRef } from 'react';
import Markdown from 'react-markdown';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { useQAComposer } from '#renderer/global/runtime/useQAComposer';
import { QAChatSurface, type QAChatSurfaceProps } from '#renderer/global/ui/QAChatSurface';
import { useMarkdownImageQAChat } from '#renderer/global/runtime/useMarkdownImageQAChat';
import { ImageQAComposer } from '#renderer/global/ui/QAChatParts';

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
  const localScrollRef = useRef<(() => void) | null>(null);
  const composerScrollRef = surfaceProps.scrollRef ?? localScrollRef;
  const {
    question,
    textareaRef,
    handleChange,
    doSubmit,
    image,
    preview,
    setImageFile,
    removeImage,
    fileRef,
  } = useMarkdownImageQAChat(onAsk, pending, composerScrollRef);

  return (
    <QAChatSurface
      {...surfaceProps}
      pending={pending}
      placeholder={placeholder}
      scrollRef={composerScrollRef}
      AssistantMessage={MarkdownAssistantMessage}
      composer={
        <ImageQAComposer
          question={question}
          textareaRef={textareaRef}
          handleChange={handleChange}
          doSubmit={doSubmit}
          pending={pending}
          image={image}
          preview={preview}
          setImageFile={setImageFile}
          removeImage={removeImage}
          fileRef={fileRef}
          placeholder={placeholder}
          formClassName={formClassName}
          formStyle={formStyle}
        />
      }
    />
  );
}
