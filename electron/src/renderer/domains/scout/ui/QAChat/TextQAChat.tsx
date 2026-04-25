import React, { useCallback, useRef } from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { useQAComposer } from '#renderer/domains/scout/runtime/useQAComposer';
import { QAChatView, type QAChatFrameProps } from '#renderer/domains/scout/ui/QAChatView';

interface SharedQAChatProps extends Omit<QAChatFrameProps, 'composer' | 'AssistantMessage'> {
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
    <QAChatView
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
