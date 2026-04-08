import React, { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, endAskSession } from '#renderer/domains/captain/hooks/useApi';
import { QAChat, type QAEntry } from '#renderer/global/components/QAChat';
import type { TaskItem } from '#renderer/types';
import { prLabel, prHref, getErrorMessage } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';

interface Props {
  item: TaskItem;
  onBack: () => void;
}

export function TaskAsk({ item, onBack }: Props): React.ReactElement {
  const [history, setHistory] = useState<QAEntry[]>([]);
  const [pending, setPending] = useState(false);
  const scrollRef = useRef<(() => void) | null>(null);
  const queryClient = useQueryClient();

  const handleAsk = useCallback(
    async (q: string) => {
      setHistory((prev) => [...prev, { role: 'user', text: q }]);
      setPending(true);
      try {
        const data = await askTask(item.id, q);
        setHistory((prev) => [...prev, { role: 'assistant', text: data.answer }]);
        void queryClient.invalidateQueries({ queryKey: ['tasks'] });
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
    [item.id, queryClient],
  );

  const [endingSession, setEndingSession] = useState(false);
  const handleEndSession = useCallback(async () => {
    setEndingSession(true);
    try {
      await endAskSession(item.id);
    } catch (err) {
      log.warn('[TaskAsk] end session failed:', err);
    } finally {
      setEndingSession(false);
      void queryClient.invalidateQueries({ queryKey: ['tasks'] });
    }
  }, [item.id, queryClient]);

  const hasAskSession = !!item.session_ids?.ask;

  const header = (
    <div className="mb-3 flex items-center gap-2">
      <Button variant="ghost" size="xs" onClick={onBack}>
        &larr; Back
      </Button>
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
              onClick={() => void handleEndSession()}
              disabled={endingSession}
              className={item.pr_number ? '' : 'ml-auto'}
            >
              {endingSession ? '...' : 'End session'}
            </Button>
          </TooltipTrigger>
          <TooltipContent>End ask session (next question starts fresh)</TooltipContent>
        </Tooltip>
      )}
    </div>
  );

  return (
    <QAChat
      testId="task-ask"
      className="min-h-[60vh]"
      header={header}
      history={history}
      pending={pending}
      scrollRef={scrollRef}
      onAsk={(q) => void handleAsk(q)}
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
