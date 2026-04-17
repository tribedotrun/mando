import React from 'react';
import { MessageSquare, RotateCcw } from 'lucide-react';
import { cn } from '#renderer/global/service/cn';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import type { AskHistoryEntry } from '#renderer/global/types';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';

export function MessageBlock({ entry }: { entry: AskHistoryEntry }): React.ReactElement {
  const isHuman = entry.role === 'human';
  const isError = entry.role === 'error';
  const intent = isHuman ? entry.intent : undefined;
  const intentLabel = intent === 'reopen' ? 'Reopen' : intent === 'rework' ? 'Rework' : null;
  const time = formatEventTime(entry.timestamp);
  const bubbleClass = isHuman
    ? intentLabel
      ? 'bg-accent/10 text-text-1 border border-accent/40'
      : 'bg-accent/10 text-text-1'
    : isError
      ? 'border border-destructive/30 bg-destructive/5 text-destructive'
      : 'bg-surface-1 text-text-1';
  const label = isHuman ? 'You' : isError ? 'Error' : 'Advisor';
  return (
    <div className={cn('px-3 py-2', isHuman ? 'flex justify-end' : '')}>
      <div className={cn('max-w-[85%] rounded-lg px-4 py-3', bubbleClass)}>
        <div className="mb-1 flex items-center gap-2">
          {intentLabel ? (
            <RotateCcw size={12} className="text-accent" />
          ) : (
            <MessageSquare size={12} className="text-text-3" />
          )}
          <span className="text-caption text-text-3">
            {intentLabel ? (
              <>
                <span className="font-medium text-accent">{intentLabel}</span>
                <span> · </span>
              </>
            ) : null}
            {label} · {time}
          </span>
        </div>
        <div className="text-body">
          <PrMarkdown text={entry.content} />
        </div>
      </div>
    </div>
  );
}
