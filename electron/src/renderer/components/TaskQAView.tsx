import React, { useCallback, useImperativeHandle, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, fetchAskHistory } from '#renderer/api';
import { PrMarkdown } from '#renderer/components/PrMarkdown';
import type { AskHistoryEntry, TaskItem } from '#renderer/types';
import { getErrorMessage, shortTs } from '#renderer/utils';

export interface QAHandle {
  ask: (question: string) => void;
}

/* ── Active Q&A View (replaces tabs when asking) ── */

interface ActiveQAProps {
  item: TaskItem;
  qaRef: React.RefObject<QAHandle | null>;
  onBack: () => void;
  /** Question passed from the action bar — consumed on mount. */
  pendingQuestion?: string | null;
  onPendingConsumed?: () => void;
}

export function ActiveQAView({
  item,
  qaRef,
  onBack,
  pendingQuestion,
  onPendingConsumed,
}: ActiveQAProps): React.ReactElement {
  const [localMessages, setLocalMessages] = useState<AskHistoryEntry[]>([]);
  const [pending, setPending] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();

  const { data: serverHistory } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
  });

  const serverMessages = serverHistory?.history ?? [];
  const serverMessagesRef = useRef(serverMessages);
  serverMessagesRef.current = serverMessages;

  // Clear optimistic local cache once server catches up.
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
        log.warn('[ActiveQAView] ask failed:', err);
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

  useImperativeHandle(qaRef, () => ({ ask: doAsk }), [doAsk]);

  // Consume pending question from action bar on mount.
  const consumedRef = useRef(false);
  if (pendingQuestion && !consumedRef.current) {
    consumedRef.current = true;
    // Schedule after mount so doAsk runs with valid refs.
    Promise.resolve().then(() => {
      doAsk(pendingQuestion);
      onPendingConsumed?.();
    });
  }

  return (
    <>
      {/* Back link */}
      <button
        onClick={onBack}
        className="mb-3 flex shrink-0 items-center gap-1.5 text-caption"
        style={{
          color: 'var(--color-text-3)',
          background: 'none',
          border: 'none',
          cursor: 'pointer',
        }}
      >
        &larr; Back to task
      </button>

      {/* Messages — scrollable */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto pr-2">
        {messages.length === 0 && !pending && (
          <div className="py-8 text-center text-caption" style={{ color: 'var(--color-text-3)' }}>
            Ask a question about this task
          </div>
        )}
        {messages.map((entry, i) => (
          <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
        ))}
        {pending && (
          <div className="py-3 text-caption" style={{ color: 'var(--color-text-3)' }}>
            Thinking...
          </div>
        )}
      </div>
    </>
  );
}

/* ── Q&A History Tab (read-only view in tabs) ── */

export function QAHistoryTab({ item }: { item: TaskItem }): React.ReactElement {
  const {
    data: serverHistory,
    isError,
    error,
  } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
  });

  const messages = serverHistory?.history ?? [];

  if (isError) {
    return (
      <div className="text-caption" style={{ color: 'var(--color-error)' }}>
        Failed to load Q&A history{error instanceof Error ? `: ${error.message}` : ''}
      </div>
    );
  }

  if (messages.length === 0) {
    return (
      <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        No Q&A history yet
      </div>
    );
  }

  return (
    <div>
      {messages.map((entry, i) => (
        <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
      ))}
    </div>
  );
}

/* ── Shared message component ── */

function QAMessage({ entry }: { entry: AskHistoryEntry }): React.ReactElement {
  const isHuman = entry.role === 'human';
  const isError = !isHuman && entry.content.startsWith('Error: ');

  return (
    <div className="mb-4">
      <div className="mb-1 flex items-center gap-2">
        <span
          className="text-label font-medium uppercase tracking-wide"
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
        <span className="text-label" style={{ color: 'var(--color-text-4)' }}>
          {shortTs(entry.timestamp)}
        </span>
      </div>
      <div
        className="rounded-lg px-4 py-3 text-body leading-relaxed"
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
