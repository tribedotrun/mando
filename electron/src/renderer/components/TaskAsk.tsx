import React, { useCallback, useRef, useState } from 'react';
import { askTask } from '#renderer/api';
import { QAChat } from '#renderer/components/QAChat';
import type { QAEntry } from '#renderer/components/QAChat';
import type { TaskItem } from '#renderer/types';
import { prLabel, prHref, getErrorMessage } from '#renderer/utils';

interface Props {
  item: TaskItem;
  onBack: () => void;
}

export function TaskAsk({ item, onBack }: Props): React.ReactElement {
  const [history, setHistory] = useState<QAEntry[]>([]);
  const [pending, setPending] = useState(false);
  const scrollRef = useRef<(() => void) | null>(null);

  const handleAsk = useCallback(
    async (q: string) => {
      setHistory((prev) => [...prev, { role: 'user', text: q }]);
      setPending(true);
      try {
        const data = await askTask(item.id, q);
        setHistory((prev) => [...prev, { role: 'assistant', text: data.answer }]);
      } catch (err) {
        setHistory((prev) => [
          ...prev,
          { role: 'assistant', text: `Error: ${getErrorMessage(err, 'Failed')}` },
        ]);
      } finally {
        setPending(false);
        scrollRef.current?.();
      }
    },
    [item.id],
  );

  const header = (
    <div className="mb-3 flex items-center gap-2">
      <button
        onClick={onBack}
        className="rounded px-3 py-1 text-xs"
        style={{ color: 'var(--color-text-2)' }}
      >
        &larr; Back
      </button>
      <span className="font-mono text-xs" style={{ color: 'var(--color-accent)' }}>
        #{item.id}
      </span>
      <span
        className="truncate max-w-xs font-mono text-xs"
        style={{ color: 'var(--color-text-3)' }}
      >
        {item.title}
      </span>
      <span className="ml-1 font-mono text-[0.6rem]" style={{ color: 'var(--color-text-4)' }}>
        [{item.status}]
      </span>
      {item.pr && item.project && (
        <a
          href={prHref(item.pr, item.project)}
          target="_blank"
          rel="noopener noreferrer"
          className="ml-auto font-mono text-xs no-underline hover:underline"
          style={{ color: 'var(--color-accent)' }}
        >
          {prLabel(item.pr)}
        </a>
      )}
    </div>
  );

  return (
    <QAChat
      testId="task-ask"
      style={{ minHeight: '60vh' }}
      header={header}
      history={history}
      pending={pending}
      scrollRef={scrollRef}
      onAsk={handleAsk}
      placeholder="Ask about this item..."
      historyClassName="mb-3 rounded p-3 max-h-[55vh]"
      historyStyle={{
        border: '1px solid var(--color-border)',
        background: 'var(--color-surface-2)',
      }}
      userBubbleStyle={{
        background: 'var(--color-accent-wash)',
        border: '1px solid var(--color-accent-wash)',
        color: 'var(--color-accent-hover)',
      }}
      assistantBubbleStyle={{
        background: 'var(--color-surface-3)',
        border: '1px solid var(--color-border-subtle)',
        color: 'var(--color-text-1)',
      }}
      bubbleClassName="max-w-[85%] whitespace-pre-wrap"
    />
  );
}
