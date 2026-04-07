import React, { useCallback, useImperativeHandle, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchAskHistory } from '#renderer/domains/captain/hooks/useApi';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { useTaskAsk } from '#renderer/global/hooks/useTaskAsk';
import type { AskHistoryEntry, TaskItem } from '#renderer/types';
import { shortTs } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';

export interface QAHandle {
  ask: (question: string) => void;
}

/* -- Active Q&A View (replaces tabs when asking) -- */

interface ActiveQAProps {
  item: TaskItem;
  qaRef: React.RefObject<QAHandle | null>;
  onBack: () => void;
  /** Question passed from the action bar, consumed on mount. */
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
  const scrollRef = useRef<HTMLDivElement>(null);
  const { messages, pending, ask } = useTaskAsk(item.id);

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current;
    if (el) setTimeout(() => (el.scrollTop = el.scrollHeight), 50);
  }, []);

  const doAsk = useCallback(
    async (question: string) => {
      scrollToBottom();
      await ask(question);
      scrollToBottom();
    },
    [ask, scrollToBottom],
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
      <Button
        variant="ghost"
        size="xs"
        onClick={onBack}
        className="mb-3 shrink-0 text-muted-foreground"
      >
        &larr; Back to task
      </Button>

      {/* Messages, scrollable */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto pr-2">
        {messages.length === 0 && !pending && (
          <div className="py-8 text-center text-caption text-text-3">
            Ask a question about this task
          </div>
        )}
        {messages.map((entry, i) => (
          <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
        ))}
        {pending && <div className="py-3 text-caption text-text-3">Thinking...</div>}
      </div>
    </>
  );
}

/* -- Q&A History Tab (read-only view in tabs) -- */

export function QAHistoryTab({ item }: { item: TaskItem }): React.ReactElement {
  const {
    data: serverHistory,
    isPending,
    isError,
    error,
  } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
  });

  const messages = serverHistory?.history ?? [];

  if (isError) {
    return (
      <div className="text-caption text-destructive">
        Failed to load Q&A history{error instanceof Error ? `: ${error.message}` : ''}
      </div>
    );
  }

  if (isPending) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    );
  }

  if (messages.length === 0) {
    return <div className="text-caption text-text-3">No Q&A history yet</div>;
  }

  return (
    <div>
      {messages.map((entry, i) => (
        <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
      ))}
    </div>
  );
}

/* -- Shared message component -- */

function QAMessage({ entry }: { entry: AskHistoryEntry }): React.ReactElement {
  const isHuman = entry.role === 'human';
  const isError = !isHuman && entry.content.startsWith('Error: ');

  return (
    <div className="mb-4">
      <div className="mb-1 flex items-center gap-2">
        <span
          className="text-label"
          style={{
            color: isError
              ? 'var(--destructive)'
              : isHuman
                ? 'var(--needs-human)'
                : 'var(--primary)',
          }}
        >
          {isError ? 'Error' : isHuman ? 'You' : 'Agent'}
        </span>
        <span className="text-label text-text-4">{shortTs(entry.timestamp)}</span>
      </div>
      <div
        className="rounded-lg px-4 py-3 text-body leading-relaxed"
        style={{
          background: isError
            ? 'color-mix(in srgb, var(--destructive) 10%, transparent)'
            : isHuman
              ? 'var(--accent)'
              : 'var(--muted)',
          color: isError
            ? 'var(--destructive)'
            : isHuman
              ? 'var(--foreground)'
              : 'var(--muted-foreground)',
        }}
      >
        {isHuman ? (
          <span className="whitespace-pre-wrap">{entry.content}</span>
        ) : (
          <PrMarkdown text={entry.content} />
        )}
      </div>
    </div>
  );
}
