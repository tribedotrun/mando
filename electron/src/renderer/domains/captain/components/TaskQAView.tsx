import React, { useCallback, useRef } from 'react';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { useTaskAsk } from '#renderer/global/hooks/useTaskAsk';
import type { AskHistoryEntry, TaskItem } from '#renderer/types';
import { shortTs } from '#renderer/utils';

const SCROLL_DELAY_MS = 50;

/* -- Live Q&A Tab (replaces the old read-only history tab) -- */

interface QATabProps {
  item: TaskItem;
  /** Question injected from the action bar; consumed immediately. */
  pendingQuestion?: string | null;
  onPendingConsumed?: () => void;
}

export function QATab({
  item,
  pendingQuestion,
  onPendingConsumed,
}: QATabProps): React.ReactElement {
  const bottomRef = useRef<HTMLDivElement>(null);
  const { messages, pending, ask } = useTaskAsk(item.id);

  const scrollToBottom = useCallback(() => {
    setTimeout(() => bottomRef.current?.scrollIntoView({ behavior: 'smooth' }), SCROLL_DELAY_MS);
  }, []);

  const doAsk = useCallback(
    async (question: string) => {
      scrollToBottom();
      await ask(question);
      scrollToBottom();
    },
    [ask, scrollToBottom],
  );

  // Consume pending question from action bar on render.
  // Reset when pendingQuestion clears so repeated identical questions are not dropped.
  const consumedRef = useRef<string | null>(null);
  if (!pendingQuestion) {
    consumedRef.current = null;
  } else if (consumedRef.current !== pendingQuestion) {
    consumedRef.current = pendingQuestion;
    void Promise.resolve().then(() => {
      void doAsk(pendingQuestion).catch((err) => console.error('Ask failed', err));
      onPendingConsumed?.();
    });
  }

  return (
    <div>
      {messages.length === 0 && !pending && (
        <div className="py-8 text-center text-caption text-text-3">
          Ask a question about this task
        </div>
      )}
      {messages.map((entry, i) => (
        <QAMessage key={`${entry.timestamp}-${i}`} entry={entry} />
      ))}
      {pending && <div className="py-3 text-caption text-text-3">Thinking...</div>}
      <div ref={bottomRef} />
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
                : 'var(--success)',
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
