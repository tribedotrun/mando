import React, { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Markdown from 'react-markdown';
import { askScout, fetchScoutItem } from '#renderer/domains/scout/hooks/useApi';
import { QAChat, type QAEntry } from '#renderer/global/components/QAChat';
import { getErrorMessage } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';

import { Separator } from '#renderer/components/ui/separator';

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
    <div className="flex items-center gap-2 px-4 py-3">
      <span className="flex-1 truncate text-xs font-medium text-foreground">{title}</span>
      <Button variant="ghost" size="icon-xs" onClick={onClose}>
        &times;
      </Button>
    </div>
  );

  const footer =
    suggestions.length > 0 ? (
      <div className="flex flex-wrap gap-1.5 px-4 pb-2">
        {suggestions.map((s) => (
          <Button
            key={s}
            variant="outline"
            size="xs"
            onClick={() => void handleAsk(s)}
            disabled={pending}
            className="rounded-full"
          >
            {s}
          </Button>
        ))}
      </div>
    ) : null;

  return (
    <QAChat
      testId="scout-qa"
      className="flex h-full flex-col"
      header={
        <>
          {header}
          <Separator />
        </>
      }
      footer={footer}
      history={history}
      pending={pending}
      scrollRef={scrollRef}
      onAsk={(q) => void handleAsk(q)}
      placeholder="Ask about this article..."
      renderAnswer={(text) => <Markdown>{text}</Markdown>}
      historyClassName="px-4 py-3"
      formClassName="px-4 py-3"
      userBubbleStyle={{
        background: 'color-mix(in srgb, var(--muted-foreground) 10%, transparent)',
        color: 'var(--primary-hover)',
        whiteSpace: 'pre-wrap',
      }}
      assistantBubbleStyle={{
        background: 'color-mix(in srgb, var(--secondary) 50%, transparent)',
        color: 'var(--foreground)',
      }}
      bubbleClassName="max-w-[90%]"
    />
  );
}
