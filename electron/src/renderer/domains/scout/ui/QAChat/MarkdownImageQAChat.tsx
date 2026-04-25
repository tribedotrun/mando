import React, { useRef } from 'react';
import Markdown from 'react-markdown';
import { QAChatView, type QAChatFrameProps } from '#renderer/domains/scout/ui/QAChatView';
import { ImageQAComposer } from '#renderer/domains/scout/ui/ImageQAComposer';

function MarkdownAssistantMessage({ text }: { text: string }): React.ReactElement {
  return <Markdown>{text}</Markdown>;
}

interface SharedQAChatProps extends Omit<QAChatFrameProps, 'composer' | 'AssistantMessage'> {
  onAsk: (question: string, images?: File[]) => void;
  draftKey: string;
  formClassName?: string;
  formStyle?: React.CSSProperties;
}

export function MarkdownImageQAChat({
  onAsk,
  pending,
  placeholder,
  draftKey,
  formClassName = '',
  formStyle,
  ...surfaceProps
}: SharedQAChatProps): React.ReactElement {
  const localScrollRef = useRef<(() => void) | null>(null);
  const composerScrollRef = surfaceProps.scrollRef ?? localScrollRef;

  return (
    <QAChatView
      {...surfaceProps}
      pending={pending}
      placeholder={placeholder}
      scrollRef={composerScrollRef}
      AssistantMessage={MarkdownAssistantMessage}
      composer={
        <ImageQAComposer
          onAsk={onAsk}
          pending={pending}
          scrollRef={composerScrollRef}
          draftKey={draftKey}
          placeholder={placeholder}
          formClassName={formClassName}
          formStyle={formStyle}
        />
      }
    />
  );
}
