import React, { useState } from 'react';
import Markdown from 'react-markdown';
import { MessageCircleQuestion, Zap } from 'lucide-react';
import { useScoutItem, useScoutArticle, useScoutAct } from '#renderer/domains/scout/runtime/hooks';
import type { ScoutItem } from '#renderer/global/types';
import { getErrorMessage } from '#renderer/global/service/utils';
import { useProjects } from '#renderer/global/runtime/useProjects';
import log from '#renderer/global/service/logger';
import { Button } from '#renderer/global/ui/button';
import { Badge } from '#renderer/global/ui/badge';
import { Separator } from '#renderer/global/ui/separator';
import { Skeleton } from '#renderer/global/ui/skeleton';
import {
  isScoutItemActionable,
  scoutItemTitle,
  formatActResult,
} from '#renderer/domains/scout/service/researchHelpers';
import { ScoutSummary } from '#renderer/domains/scout/ui/ScoutSummary';
import { ScoutActForm } from '#renderer/domains/scout/ui/ScoutActForm';

interface Props {
  itemId: number;
  onAsk: () => void;
  qaOpen?: boolean;
}

// Parent should render with key={itemId} so this component remounts on item change
export function ScoutReader({ itemId, onAsk, qaOpen }: Props): React.ReactElement {
  const [summaryOpen, setSummaryOpen] = useState(true);
  const [actOpen, setActOpen] = useState(false);
  const [actProject, setActProject] = useState('');
  const [actPrompt, setActPrompt] = useState('');
  const actMut = useScoutAct();

  const projects = useProjects();

  // Derive effective project: auto-select when exactly one exists
  const effectiveActProject = actProject || (projects.length === 1 ? projects[0] : '');

  const itemQuery = useScoutItem(itemId);

  const articleQuery = useScoutArticle(itemId);

  const item: ScoutItem | null = itemQuery.data ?? null;
  const loading = itemQuery.isLoading;
  const error = itemQuery.error ? getErrorMessage(itemQuery.error, 'Failed') : null;
  const displayTitle = item ? scoutItemTitle(item) : 'Untitled';
  const article = articleQuery.data?.article ?? null;
  const articleLoading = articleQuery.isLoading;

  const actResult = formatActResult(actMut.data, actMut.error);

  const handleAct = () => {
    if (!effectiveActProject) return;
    actMut.reset();
    actMut.mutate(
      { id: itemId, project: effectiveActProject, prompt: actPrompt || undefined },
      {
        onError: (e) => log.warn('[ScoutReader] actOnScoutItem failed', { itemId, err: e }),
      },
    );
  };

  if (loading) {
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

  if (error || !item) {
    return (
      <div data-testid="scout-reader" className="h-full px-5 py-4">
        <div className="text-xs text-destructive">{error ?? `Item #${itemId} not found`}</div>
      </div>
    );
  }

  const maxWidthClass = qaOpen ? '' : 'mx-auto max-w-[720px]';

  return (
    <div data-testid="scout-reader" className="flex h-full flex-col">
      {/* Header - pinned above scroll */}
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
              <div className="flex items-center gap-3 font-mono text-xs text-muted-foreground">
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
                  onClick={() => {
                    setActOpen(!actOpen);
                    actMut.reset();
                  }}
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

      {/* Content - scrollable */}
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <div className={maxWidthClass}>
          {/* Act form */}
          {actOpen && (
            <ScoutActForm
              projects={projects}
              actProject={effectiveActProject}
              setActProject={setActProject}
              actPrompt={actPrompt}
              setActPrompt={setActPrompt}
              acting={actMut.isPending}
              actResult={actResult}
              onAct={handleAct}
            />
          )}

          {/* Summary */}
          {item.summary && (
            <ScoutSummary
              summary={item.summary}
              summaryOpen={summaryOpen}
              onToggle={() => setSummaryOpen(!summaryOpen)}
            />
          )}

          <Separator className="my-4" />

          {/* Article */}
          <div>
            {articleLoading && (
              <div className="space-y-3 py-8">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-5/6" />
                <Skeleton className="h-4 w-4/6" />
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4" />
              </div>
            )}
            {article && (
              <div className="prose-scout text-sm leading-relaxed text-foreground">
                <Markdown>{article}</Markdown>
              </div>
            )}
            {!article && !articleLoading && (
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
