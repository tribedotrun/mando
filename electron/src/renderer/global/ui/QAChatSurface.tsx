import React, { useCallback, useRef } from 'react';
import { ScrollArea } from '#renderer/global/ui/scroll-area';

const SCROLL_DELAY_MS = 50;

function PlainAssistantMessage({ text }: { text: string }): React.ReactNode {
  return text;
}

export interface QAEntry {
  role: 'user' | 'assistant';
  text: string;
}

export interface QAChatSurfaceProps {
  history: QAEntry[];
  pending: boolean;
  placeholder?: string;
  header?: React.ReactNode;
  footer?: React.ReactNode;
  scrollRef?: React.MutableRefObject<(() => void) | null>;
  testId?: string;
  className?: string;
  style?: React.CSSProperties;
  userBubbleStyle?: React.CSSProperties;
  assistantBubbleStyle?: React.CSSProperties;
  bubbleClassName?: string;
  historyClassName?: string;
  historyStyle?: React.CSSProperties;
  composer?: React.ReactNode;
  AssistantMessage?: React.ComponentType<{ text: string }>;
}

export function QAChatSurface({
  history,
  pending,
  placeholder = 'Ask a question...',
  header,
  footer,
  scrollRef,
  testId,
  className = 'flex flex-col',
  style,
  userBubbleStyle,
  assistantBubbleStyle,
  bubbleClassName = '',
  historyClassName = '',
  historyStyle,
  composer,
  AssistantMessage = PlainAssistantMessage,
}: QAChatSurfaceProps): React.ReactElement {
  const chatEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = useCallback(() => {
    setTimeout(() => chatEndRef.current?.scrollIntoView({ behavior: 'smooth' }), SCROLL_DELAY_MS);
  }, []);

  if (scrollRef) scrollRef.current = scrollToBottom;

  const defaultUserStyle: React.CSSProperties = {
    background: 'var(--accent)',
    border: '1px solid var(--accent)',
    color: 'var(--primary-hover)',
    whiteSpace: 'pre-wrap',
  };

  const defaultAssistantStyle: React.CSSProperties = {
    background: 'var(--secondary)',
    border: '1px solid var(--input)',
    color: 'var(--foreground)',
  };

  return (
    <div data-testid={testId} className={className} style={style}>
      {header}

      <ScrollArea className={`min-h-0 flex-1 ${historyClassName}`} style={historyStyle}>
        {history.length === 0 && (
          <div className="py-8 text-center text-xs text-text-3">{placeholder}</div>
        )}
        {history.map((entry, index) => (
          <div key={index} className={`mb-3 ${entry.role === 'user' ? 'text-right' : 'text-left'}`}>
            <div
              className={`inline-block rounded px-3 py-2 text-xs leading-relaxed ${bubbleClassName}`}
              style={
                entry.role === 'user'
                  ? { ...defaultUserStyle, ...userBubbleStyle }
                  : { ...defaultAssistantStyle, ...assistantBubbleStyle }
              }
            >
              {entry.role === 'assistant' ? <AssistantMessage text={entry.text} /> : entry.text}
            </div>
          </div>
        ))}
        {pending && (
          <div className="flex items-center gap-2 py-2">
            <span className="text-xs text-text-3">Thinking...</span>
          </div>
        )}
        <div ref={chatEndRef} />
      </ScrollArea>

      {footer}
      {composer}
    </div>
  );
}
