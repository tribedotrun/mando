import React, { useCallback, useImperativeHandle, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, fetchAskHistory } from '#renderer/api';
import { PrMarkdown } from '#renderer/components/PrMarkdown';
import type { AskHistoryEntry, TaskItem } from '#renderer/types';
import { getErrorMessage, shortTs } from '#renderer/utils';

/** A single Q&A message rendered in the chat list. */
function QAMessage({ entry }: { entry: AskHistoryEntry }): React.ReactElement {
  const isHuman = entry.role === 'human';
  const isError = !isHuman && entry.content.startsWith('Error: ');
  return (
    <div className="mb-3">
      <div className="mb-0.5 flex items-center gap-2">
        <span
          className="text-[10px] font-medium uppercase tracking-wide"
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
        className="rounded px-3 py-2 text-[12px] leading-relaxed"
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

/** Inline Q&A input bar — textarea + Ask button. */
function QAInput({
  onAsk,
  pending,
  placeholder,
}: {
  onAsk: (q: string) => void;
  pending: boolean;
  placeholder?: string;
}): React.ReactElement {
  const [text, setText] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const submit = useCallback(() => {
    const q = text.trim();
    if (!q || pending) return;
    onAsk(q);
    setText('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [text, pending, onAsk]);

  return (
    <div
      className="flex items-end gap-2 pt-2"
      style={{ borderTop: '1px solid var(--color-border-subtle)' }}
    >
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
        }}
        placeholder={placeholder ?? 'Follow-up question...'}
        className="flex-1 resize-none rounded px-3 py-2 text-[12px] placeholder-[var(--color-text-3)] focus:outline-none"
        style={{
          border: '1px solid var(--color-border-subtle)',
          background: 'var(--color-surface-2)',
          color: 'var(--color-text-1)',
        }}
        rows={1}
        disabled={pending}
      />
      <button
        onClick={submit}
        disabled={pending || !text.trim()}
        className="rounded-md px-3 py-1.5 text-[11px] font-medium disabled:opacity-40"
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
  );
}

// ── Exported components ──

export interface TaskQAHandle {
  askFromBar: (question: string) => void;
  getMessages: () => AskHistoryEntry[];
}

interface TaskQAProps {
  item: TaskItem;
  /** Ref exposed to parent so the action bar can trigger asks. */
  qaRef: React.RefObject<TaskQAHandle | null>;
  onExpand: () => void;
  onClose: () => void;
}

export function TaskQA({ item, qaRef, onExpand, onClose }: TaskQAProps): React.ReactElement | null {
  const [localMessages, setLocalMessages] = useState<AskHistoryEntry[]>([]);
  const [pending, setPending] = useState(false);
  const [minimized, setMinimized] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();

  const {
    data: serverHistory,
    isError,
    error,
  } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
  });

  const serverMessages = serverHistory?.history ?? [];
  const serverMessagesRef = useRef(serverMessages);
  serverMessagesRef.current = serverMessages;

  // Clear optimistic local cache once server catches up (e.g. after expanded view adds messages).
  if (localMessages.length > 0 && serverMessages.length >= localMessages.length) {
    setLocalMessages([]);
  }
  const messages = localMessages.length > 0 ? localMessages : serverMessages;

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current;
    if (el) setTimeout(() => (el.scrollTop = el.scrollHeight), 50);
  }, []);

  const doAsk = useCallback(
    async (question: string) => {
      const now = new Date().toISOString();
      const userEntry: AskHistoryEntry = { role: 'human', content: question, timestamp: now };
      setLocalMessages((prev) => {
        const base = prev.length > 0 ? prev : serverMessagesRef.current;
        return [...base, userEntry];
      });
      setPending(true);
      scrollToBottom();
      try {
        const data = await askTask(item.id, question);
        setLocalMessages((prev) => [
          ...prev,
          { role: 'assistant', content: data.answer, timestamp: new Date().toISOString() },
        ]);
        queryClient.invalidateQueries({ queryKey: ['task-ask-history', item.id] });
      } catch (err) {
        log.warn('[TaskQA] ask failed:', err);
        setLocalMessages((prev) => [
          ...prev,
          {
            role: 'assistant',
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

  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  useImperativeHandle(
    qaRef,
    () => ({ askFromBar: doAsk, getMessages: () => messagesRef.current }),
    [doAsk],
  );

  if (messages.length === 0 && !pending && !isError) return null;

  const msgCount = messages.length;

  return (
    <div className="shrink-0" style={{ borderTop: '1px solid var(--color-border-subtle)' }}>
      {/* Header */}
      <div className="flex items-center gap-2 px-1 py-2">
        <button
          onClick={() => setMinimized((v) => !v)}
          className="flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-widest"
          style={{
            color: 'var(--color-text-4)',
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            padding: 0,
          }}
        >
          <svg
            width="8"
            height="8"
            viewBox="0 0 8 8"
            fill="currentColor"
            style={{
              transition: 'transform 150ms',
              transform: minimized ? 'none' : 'rotate(90deg)',
            }}
          >
            <path d="M2 1l4 3-4 3V1z" />
          </svg>
          Q&A
          {msgCount > 0 && (
            <span style={{ fontWeight: 400, letterSpacing: 'normal', textTransform: 'none' }}>
              ({msgCount})
            </span>
          )}
          {pending && (
            <span
              style={{
                fontWeight: 400,
                letterSpacing: 'normal',
                textTransform: 'none',
                color: 'var(--color-accent)',
              }}
            >
              thinking...
            </span>
          )}
        </button>
        <span className="flex-1" />
        <button
          onClick={onExpand}
          className="flex items-center justify-center rounded"
          style={{
            width: 24,
            height: 24,
            background: 'none',
            border: '1px solid var(--color-border-subtle)',
            color: 'var(--color-text-3)',
            cursor: 'pointer',
          }}
          title="Expand to full page"
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 12 12"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.3"
            strokeLinecap="round"
          >
            <path d="M7 1h4v4M5 11H1V7M11 1L6.5 5.5M1 11l4.5-4.5" />
          </svg>
        </button>
        <button
          onClick={onClose}
          className="rounded px-1.5 py-0.5 text-[11px]"
          style={{
            background: 'none',
            border: '1px solid var(--color-border-subtle)',
            color: 'var(--color-text-3)',
            cursor: 'pointer',
          }}
          title="Close Q&A"
        >
          Close
        </button>
      </div>

      {/* Messages + Input — hidden when minimized */}
      {!minimized && (
        <>
          <div ref={scrollRef} className="overflow-auto px-1" style={{ maxHeight: 300 }}>
            {isError && messages.length === 0 && (
              <div className="py-2 text-[11px]" style={{ color: 'var(--color-error)' }}>
                Failed to load history{error instanceof Error ? `: ${error.message}` : ''}
              </div>
            )}
            {messages.map((entry, i) => (
              <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
            ))}
            {pending && (
              <div className="py-2 text-[11px]" style={{ color: 'var(--color-text-3)' }}>
                Thinking...
              </div>
            )}
          </div>
          <div className="px-1 pb-2">
            <QAInput onAsk={doAsk} pending={pending} />
          </div>
        </>
      )}
    </div>
  );
}
