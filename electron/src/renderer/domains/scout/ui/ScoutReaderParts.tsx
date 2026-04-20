import React from 'react';
import { ExternalLink, MessageCircleQuestion, Send, Zap } from 'lucide-react';
import type { ScoutItem } from '#renderer/global/types';
import { isScoutItemActionable } from '#renderer/domains/scout/service/researchHelpers';
import { Badge } from '#renderer/global/ui/badge';
import { Button } from '#renderer/global/ui/button';
import { Skeleton } from '#renderer/global/ui/skeleton';

interface ScoutReaderHeaderProps {
  item: ScoutItem;
  displayTitle: string;
  qaOpen: boolean | undefined;
  actOpen: boolean;
  canPublishTelegraph: boolean;
  publishingTelegraph: boolean;
  onAsk: () => void;
  onToggleAct: () => void;
  onPublishTelegraph: () => void;
  maxWidthClass: string;
}

export function ScoutReaderHeader({
  item,
  displayTitle,
  qaOpen,
  actOpen,
  canPublishTelegraph,
  publishingTelegraph,
  onAsk,
  onToggleAct,
  onPublishTelegraph,
  maxWidthClass,
}: ScoutReaderHeaderProps): React.ReactElement {
  return (
    <div className="shrink-0 border-b border-border bg-background px-5 pb-3 pt-4">
      <div className={maxWidthClass}>
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="mb-1.5 text-lg font-semibold leading-snug">
              {item.url ? (
                <a
                  href={item.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-foreground hover:text-foreground/80 hover:underline"
                >
                  <span className="line-clamp-2">{displayTitle}</span>
                </a>
              ) : (
                <span className="line-clamp-2">{displayTitle}</span>
              )}
            </h1>
            <div className="flex flex-wrap items-center gap-2.5 font-mono text-xs text-muted-foreground">
              <Badge variant="outline" className="text-[11px]">
                {item.item_type ?? 'blog'}
              </Badge>
              {item.relevance != null && item.quality != null && (
                <span>
                  R:{item.relevance} Q:{item.quality}
                </span>
              )}
              {item.source_name && <span>{item.source_name}</span>}
              {item.date_published && <span>{item.date_published}</span>}
              {item.telegraphUrl ? (
                <Button asChild variant="outline" size="xs">
                  <a
                    href={item.telegraphUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    aria-label="Open Telegraph page"
                    title="Open Telegraph page"
                  >
                    <ExternalLink size={12} />
                    Telegraph
                  </a>
                </Button>
              ) : canPublishTelegraph ? (
                <Button
                  variant="outline"
                  size="xs"
                  onClick={onPublishTelegraph}
                  disabled={publishingTelegraph}
                  aria-label="Publish to Telegraph"
                  title="Publish to Telegraph"
                >
                  <Send size={12} />
                  {publishingTelegraph ? 'Publishing...' : 'Publish'}
                </Button>
              ) : null}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1 pt-1">
            <Button
              variant={qaOpen ? 'secondary' : 'ghost'}
              size="icon-sm"
              onClick={onAsk}
              title="Ask about this item"
              aria-label="Ask about this item"
            >
              <MessageCircleQuestion size={16} />
            </Button>
            {isScoutItemActionable(item) && (
              <Button
                variant={actOpen ? 'secondary' : 'ghost'}
                size="icon-sm"
                onClick={onToggleAct}
                title="Create task from this item"
                aria-label="Create task from this item"
              >
                <Zap size={16} />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export function ScoutReaderSkeleton(): React.ReactElement {
  return (
    <div data-testid="scout-reader" className="h-full px-5 py-4">
      <div className="mx-auto max-w-[720px] space-y-4 py-8">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    </div>
  );
}
