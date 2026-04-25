import React from 'react';
import Markdown from 'react-markdown';
import { useScoutReader } from '#renderer/domains/scout/runtime/useScoutReader';
import { ScoutSummary } from '#renderer/domains/scout/ui/ScoutSummary';
import { ScoutActForm } from '#renderer/domains/scout/ui/ScoutActForm';
import {
  ScoutReaderHeader,
  ScoutReaderSkeleton,
} from '#renderer/domains/scout/ui/ScoutReaderParts';
import { Separator } from '#renderer/global/ui/primitives/separator';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

interface Props {
  itemId: number;
  onAsk: () => void;
  qaOpen?: boolean;
}

// Parent should render with key={itemId} so this component remounts on item change
export function ScoutReader({ itemId, onAsk, qaOpen }: Props): React.ReactElement {
  const reader = useScoutReader({ itemId });

  if (reader.item.loading) {
    return <ScoutReaderSkeleton />;
  }

  if (reader.item.error || !reader.item.value) {
    return (
      <div data-testid="scout-reader" className="h-full px-5 py-4">
        <div className="text-xs text-destructive">
          {reader.item.error ?? `Item #${itemId} not found`}
        </div>
      </div>
    );
  }

  const maxWidthClass = qaOpen ? '' : 'mx-auto max-w-[720px]';
  const item = reader.item.value;

  return (
    <div data-testid="scout-reader" className="flex h-full flex-col">
      {/* Header - pinned above scroll */}
      <ScoutReaderHeader
        item={item}
        displayTitle={reader.item.displayTitle}
        qaOpen={qaOpen}
        actOpen={reader.act.open}
        canPublishTelegraph={!!reader.article.body}
        publishingTelegraph={reader.publish.telegraphPending}
        onAsk={onAsk}
        onToggleAct={() => reader.act.setOpen(!reader.act.open)}
        onPublishTelegraph={reader.publish.handleTelegraph}
        maxWidthClass={maxWidthClass}
      />

      {/* Content - scrollable */}
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <div className={maxWidthClass}>
          {/* Act form */}
          <div className={reader.act.open ? '' : 'hidden'}>
            <ScoutActForm itemId={itemId} open={reader.act.open} />
          </div>

          {/* Summary */}
          {item.summary && (
            <ScoutSummary
              summary={item.summary}
              summaryOpen={reader.summary.open}
              onToggle={() => reader.summary.setOpen(!reader.summary.open)}
            />
          )}

          <Separator className="my-4" />

          {/* Article */}
          <div>
            {reader.article.loading && (
              <div className="space-y-3 py-8">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-5/6" />
                <Skeleton className="h-4 w-4/6" />
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4" />
              </div>
            )}
            {reader.article.body && (
              <div className="prose-scout text-sm leading-relaxed text-foreground">
                <Markdown>{reader.article.body}</Markdown>
              </div>
            )}
            {!reader.article.body && !reader.article.loading && (
              <div className="py-8 text-center text-xs text-muted-foreground">
                No article content. Process the item first.
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
