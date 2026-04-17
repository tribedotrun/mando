import React, { useCallback, useMemo, useRef } from 'react';
import { useEndAskSession } from '#renderer/domains/captain/runtime/hooks';
import { useTaskAsk } from '#renderer/domains/captain/runtime/useTaskAsk';
import { TextQAChat, type QAEntry } from '#renderer/global/ui/QAChat';
import type { TaskItem } from '#renderer/global/types';
import { prLabel, prHref } from '#renderer/global/service/utils';
import { Button } from '#renderer/global/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/global/ui/tooltip';

interface Props {
  item: TaskItem;
}

export function TaskAsk({ item }: Props): React.ReactElement {
  const scrollRef = useRef<(() => void) | null>(null);
  const { messages, pending, ask } = useTaskAsk(item.id);
  const endMut = useEndAskSession();

  const history = useMemo<QAEntry[]>(
    () =>
      messages.map((m) => ({
        role: m.role === 'human' ? 'user' : 'assistant',
        text: m.content,
      })),
    [messages],
  );

  const handleAsk = (q: string) => {
    void ask(q).then(() => scrollRef.current?.());
  };

  const handleEndSession = useCallback(() => {
    endMut.mutate({ id: item.id });
  }, [item.id, endMut]);

  const hasAskSession = !!item.session_ids?.ask;

  const header = (
    <div className="mb-3 flex items-center gap-2">
      <span className="font-mono text-xs text-muted-foreground">#{item.id}</span>
      <span className="max-w-xs truncate font-mono text-xs text-text-3">{item.title}</span>
      <span className="ml-1 font-mono text-[0.6rem] text-text-4">[{item.status}]</span>
      {item.pr_number && (item.github_repo || item.project) && (
        <a
          href={prHref(item.pr_number, (item.github_repo ?? item.project)!)}
          target="_blank"
          rel="noopener noreferrer"
          className="ml-auto font-mono text-xs text-muted-foreground no-underline hover:underline"
        >
          {prLabel(item.pr_number)}
        </a>
      )}
      {hasAskSession && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="xs"
              onClick={handleEndSession}
              disabled={endMut.isPending}
              className={item.pr_number ? '' : 'ml-auto'}
            >
              {endMut.isPending ? '...' : 'End session'}
            </Button>
          </TooltipTrigger>
          <TooltipContent>End ask session (next question starts fresh)</TooltipContent>
        </Tooltip>
      )}
    </div>
  );

  return (
    <TextQAChat
      testId="task-ask"
      className="min-h-[60vh]"
      header={header}
      history={history}
      pending={pending}
      scrollRef={scrollRef}
      onAsk={handleAsk}
      placeholder="Ask about this item..."
      historyClassName="mb-3 rounded p-3 max-h-[55vh]"
      historyStyle={{
        border: '1px solid var(--border)',
        background: 'var(--muted)',
      }}
      userBubbleStyle={{
        background: 'var(--accent)',
        border: '1px solid var(--accent)',
        color: 'var(--primary-hover)',
      }}
      assistantBubbleStyle={{
        background: 'var(--secondary)',
        border: '1px solid var(--input)',
        color: 'var(--foreground)',
      }}
      bubbleClassName="max-w-[85%] whitespace-pre-wrap"
    />
  );
}
