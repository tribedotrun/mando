import React, { useRef } from 'react';
import { useScoutItem, useScoutQASession } from '#renderer/domains/scout/runtime/hooks';
import { scoutItemTitle } from '#renderer/domains/scout/service/researchHelpers';
import { MarkdownImageQAChat } from '#renderer/global/ui/QAChat';
import { Button } from '#renderer/global/ui/button';
import { Separator } from '#renderer/global/ui/separator';

interface Props {
  itemId: number;
  onClose: () => void;
}

export function ScoutQA({ itemId, onClose }: Props): React.ReactElement {
  const scrollRef = useRef<(() => void) | null>(null);
  const { data: item = null } = useScoutItem(itemId);
  const { history, pending, suggestions, ask } = useScoutQASession(itemId);

  const handleAsk = (q: string, images?: File[]) => {
    void ask(q, images).then(() => scrollRef.current?.());
  };

  const title = item ? scoutItemTitle(item) : 'Untitled';

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
    <MarkdownImageQAChat
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
      onAsk={(q, images) => void handleAsk(q, images)}
      placeholder="Ask about this article..."
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
