import React, { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Markdown from 'react-markdown';
import { askScout, fetchScoutItem } from '#renderer/api';
import { QAChat } from '#renderer/components/QAChat';
import type { QAEntry } from '#renderer/components/QAChat';
import { getErrorMessage } from '#renderer/utils';

interface Props {
  itemId: number;
  onClose: () => void;
}

export function ScoutQA({ itemId, onClose }: Props): React.ReactElement {
  const [history, setHistory] = useState<QAEntry[]>([]);
  const [pending, setPending] = useState(false);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const scrollRef = useRef<(() => void) | null>(null);
  const sessionIdRef = useRef<string | undefined>(undefined);

  const { data: item = null } = useQuery({
    queryKey: ['scout', 'item', itemId],
    queryFn: () => fetchScoutItem(itemId),
  });

  const handleAsk = useCallback(
    async (q: string) => {
      setHistory((prev) => [...prev, { role: 'user', text: q }]);
      setPending(true);
      setSuggestions([]);
      try {
        const data = await askScout(itemId, q, sessionIdRef.current);
        sessionIdRef.current = data.session_id;
        setHistory((prev) => [...prev, { role: 'assistant', text: data.answer }]);
        if (data.suggested_followups?.length) {
          setSuggestions(data.suggested_followups);
        }
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
    [itemId],
  );

  const title =
    item?.title || (item?.status === 'pending' ? 'Pending processing\u2026' : 'Untitled');

  const header = (
    <div
      className="flex items-center gap-2 border-b px-4 py-3"
      style={{ borderColor: 'var(--color-border)' }}
    >
      <span
        className="text-xs font-medium truncate flex-1"
        style={{ color: 'var(--color-text-1)' }}
      >
        {title}
      </span>
      <button
        onClick={onClose}
        className="rounded p-1"
        style={{ color: 'var(--color-text-3)' }}
        title="Close Q&A"
      >
        &times;
      </button>
    </div>
  );

  const footer =
    suggestions.length > 0 ? (
      <div className="flex flex-wrap gap-1.5 px-4 pb-2">
        {suggestions.map((s) => (
          <button
            key={s}
            onClick={() => handleAsk(s)}
            disabled={pending}
            className="rounded-full px-2.5 py-1 text-xs transition-colors"
            style={{
              background: 'color-mix(in srgb, var(--color-accent) 10%, transparent)',
              color: 'var(--color-accent)',
              border: '1px solid color-mix(in srgb, var(--color-accent) 20%, transparent)',
            }}
          >
            {s}
          </button>
        ))}
      </div>
    ) : null;

  return (
    <QAChat
      testId="scout-qa"
      className="flex h-full flex-col"
      header={header}
      footer={footer}
      history={history}
      pending={pending}
      scrollRef={scrollRef}
      onAsk={handleAsk}
      placeholder="Ask about this article..."
      renderAnswer={(text) => <Markdown>{text}</Markdown>}
      historyClassName="px-4 py-3"
      formClassName="border-t px-4 py-3"
      formStyle={{ borderColor: 'var(--color-border)' }}
      userBubbleStyle={{
        background: 'color-mix(in srgb, var(--color-accent) 15%, transparent)',
        borderWidth: 1,
        borderStyle: 'solid',
        borderColor: 'color-mix(in srgb, var(--color-accent) 30%, transparent)',
        color: 'var(--color-accent-hover)',
        whiteSpace: 'pre-wrap',
      }}
      assistantBubbleStyle={{
        background: 'color-mix(in srgb, var(--color-surface-3) 50%, transparent)',
        borderWidth: 1,
        borderStyle: 'solid',
        borderColor: 'color-mix(in srgb, var(--color-border) 50%, transparent)',
        color: 'var(--color-text-1)',
      }}
      bubbleClassName="max-w-[90%]"
    />
  );
}
