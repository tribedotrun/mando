import React, { useCallback, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, fetchAskHistory } from '#renderer/api';
import { PrMarkdown } from '#renderer/components/PrMarkdown';
import type { AskHistoryEntry, TaskItem } from '#renderer/types';
import { getErrorMessage, shortTs } from '#renderer/utils';

/** A single Q&A message rendered in the expanded view. */
function ExpandedMessage({ entry }: { entry: AskHistoryEntry }): React.ReactElement {
  const isHuman = entry.role === 'human';
  const isError = !isHuman && entry.content.startsWith('Error: ');
  return (
    <div className="mb-4">
      <div className="mb-1 flex items-center gap-2">
        <span
          className="text-[11px] font-medium uppercase tracking-wide"
          style={{
            color: isError
              ? 'var(--color-error)'
              : isHuman
                ? 'var(--color-needs-human)'
                : 'var(--color-accent)',
          }}
        >
          {isError ? 'Error' : isHuman ? 'You' : 'Agent'}
        </span>
        <span className="text-[10px]" style={{ color: 'var(--color-text-4)' }}>
          {shortTs(entry.timestamp)}
        </span>
      </div>
      <div
        className="rounded-lg px-4 py-3 text-[13px] leading-relaxed"
        style={{
          background: isError
            ? 'color-mix(in srgb, var(--color-error) 10%, transparent)'
            : isHuman
              ? 'var(--color-accent-wash)'
              : 'var(--color-surface-2)',
          color: isError
            ? 'var(--color-error)'
            : isHuman
              ? 'var(--color-text-1)'
              : 'var(--color-text-2)',
        }}
      >
        {isHuman ? (
          <span style={{ whiteSpace: 'pre-wrap' }}>{entry.content}</span>
        ) : (
          <PrMarkdown text={entry.content} />
        )}
      </div>
    </div>
  );
}

interface Props {
  item: TaskItem;
  /** Initial messages to display (avoids re-fetch if already loaded). */
  initialMessages?: AskHistoryEntry[];
  /** Whether this is read-only (reviewing a closed conversation). */
  readOnly?: boolean;
  onBack: () => void;
  onClose?: () => void;
}

export function TaskQAExpanded({
  item,
  initialMessages,
  readOnly,
  onBack,
  onClose,
}: Props): React.ReactElement {
  const [localMessages, setLocalMessages] = useState<AskHistoryEntry[]>([]);
  const [pending, setPending] = useState(false);
  const [text, setText] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const queryClient = useQueryClient();

  const {
    data: serverHistory,
    isError,
    error,
  } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
    enabled: !initialMessages,
  });

  const serverMessages = initialMessages ?? serverHistory?.history ?? [];
  const serverMessagesRef = useRef(serverMessages);
  serverMessagesRef.current = serverMessages;
  const messages = localMessages.length > 0 ? localMessages : serverMessages;

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current;
    if (el) setTimeout(() => (el.scrollTop = el.scrollHeight), 50);
  }, []);

  const doAsk = useCallback(
    async (question: string) => {
      const now = new Date().toISOString();
      setLocalMessages((prev) => {
        const base = prev.length > 0 ? prev : serverMessagesRef.current;
        return [...base, { role: 'human' as const, content: question, timestamp: now }];
      });
      setPending(true);
      scrollToBottom();
      try {
        const data = await askTask(item.id, question);
        setLocalMessages((prev) => [
          ...prev,
          { role: 'assistant' as const, content: data.answer, timestamp: new Date().toISOString() },
        ]);
        queryClient.invalidateQueries({ queryKey: ['task-ask-history', item.id] });
      } catch (err) {
        log.warn('[TaskQAExpanded] ask failed:', err);
        setLocalMessages((prev) => [
          ...prev,
          {
            role: 'assistant' as const,
            content: `Error: ${getErrorMessage(err, 'Failed to get answer')}`,
            timestamp: new Date().toISOString(),
          },
        ]);
      } finally {
        setPending(false);
        scrollToBottom();
      }
    },
    [item.id, scrollToBottom, queryClient],
  );

  const submit = useCallback(() => {
    const q = text.trim();
    if (!q || pending) return;
    doAsk(q);
    setText('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [text, pending, doAsk]);

  return (
    <div className="fixed inset-0 z-[300] flex flex-col" style={{ background: 'var(--color-bg)' }}>
      {/* Header */}
      <div
        className="flex shrink-0 items-center gap-3 px-6 py-4"
        style={{ borderBottom: '1px solid var(--color-border-subtle)' }}
      >
        <button
          onClick={onBack}
          className="flex items-center gap-1.5 text-[12px]"
          style={{
            color: 'var(--color-text-3)',
            background: 'none',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          &larr; Back to task
        </button>
        <span className="flex-1" />
        <span className="text-[12px] font-medium" style={{ color: 'var(--color-text-2)' }}>
          Q&A &middot; {item.title}
        </span>
        <span className="flex-1" />
        {onClose && !readOnly && (
          <button
            onClick={onClose}
            className="rounded px-2 py-1 text-[11px]"
            style={{
              background: 'none',
              border: '1px solid var(--color-border-subtle)',
              color: 'var(--color-text-3)',
              cursor: 'pointer',
            }}
          >
            Close Q&A
          </button>
        )}
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto px-6 py-4">
        <div className="mx-auto max-w-[720px]">
          {isError && messages.length === 0 && (
            <div className="py-12 text-center text-[12px]" style={{ color: 'var(--color-error)' }}>
              Failed to load history{error instanceof Error ? `: ${error.message}` : ''}
            </div>
          )}
          {!isError && messages.length === 0 && (
            <div className="py-12 text-center text-[12px]" style={{ color: 'var(--color-text-3)' }}>
              No messages yet
            </div>
          )}
          {messages.map((entry, i) => (
            <ExpandedMessage key={`${entry.timestamp}-${i}`} entry={entry} />
          ))}
          {pending && (
            <div className="py-3 text-[12px]" style={{ color: 'var(--color-text-3)' }}>
              Thinking...
            </div>
          )}
        </div>
      </div>

      {/* Input bar (hidden in read-only mode) */}
      {!readOnly && (
        <div
          className="shrink-0 px-6 py-3"
          style={{ borderTop: '1px solid var(--color-border-subtle)' }}
        >
          <div className="mx-auto flex max-w-[720px] items-end gap-2">
            <textarea
              ref={textareaRef}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                e.target.style.height = 'auto';
                e.target.style.height = e.target.scrollHeight + 'px';
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  submit();
                }
                if (e.key === 'Escape') {
                  e.preventDefault();
                  onBack();
                }
              }}
              placeholder="Ask a follow-up question..."
              className="flex-1 resize-none rounded-lg px-4 py-2.5 text-[13px] placeholder-[var(--color-text-3)] focus:outline-none"
              style={{
                border: '1px solid var(--color-border)',
                background: 'var(--color-surface-2)',
                color: 'var(--color-text-1)',
              }}
              rows={1}
              disabled={pending}
              autoFocus
            />
            <button
              onClick={submit}
              disabled={pending || !text.trim()}
              className="rounded-md px-4 py-2 text-[12px] font-medium disabled:opacity-40"
              style={{
                background: 'var(--color-accent)',
                color: 'var(--color-bg)',
                border: 'none',
                cursor: pending || !text.trim() ? 'default' : 'pointer',
              }}
            >
              {pending ? '...' : 'Ask'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
