import React from 'react';
import Markdown from 'react-markdown';
import { useScoutReader } from '#renderer/domains/scout/runtime/useScoutReader';
import { ScoutSummary } from '#renderer/domains/scout/ui/ScoutSummary';
import { ScoutActForm } from '#renderer/domains/scout/ui/ScoutActForm';
import {
  ScoutReaderHeader,
  ScoutReaderSkeleton,
} from '#renderer/domains/scout/ui/ScoutReaderParts';
import { Separator } from '#renderer/global/ui/separator';
import { Skeleton } from '#renderer/global/ui/skeleton';

interface Props {
  itemId: number;
  onAsk: () => void;
  qaOpen?: boolean;
}

// Parent should render with key={itemId} so this component remounts on item change
export function ScoutReader({ itemId, onAsk, qaOpen }: Props): React.ReactElement {
  const {
    item,
    loading,
    error,
    displayTitle,
    article,
    articleLoading,
    projects,
    actOpen,
    setActOpen,
    summaryOpen,
    setSummaryOpen,
    setActProject,
    effectiveActProject,
    actPrompt,
    setActPrompt,
    acting,
    actResult,
    resetAct,
    handleAct,
    publishingTelegraph,
    handlePublishTelegraph,
  } = useScoutReader({ itemId });

  if (loading) {
    return <ScoutReaderSkeleton />;
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
      <ScoutReaderHeader
        item={item}
        displayTitle={displayTitle}
        qaOpen={qaOpen}
        actOpen={actOpen}
        canPublishTelegraph={!!article}
        publishingTelegraph={publishingTelegraph}
        onAsk={onAsk}
        onToggleAct={() => {
          setActOpen(!actOpen);
          resetAct();
        }}
        onPublishTelegraph={handlePublishTelegraph}
        maxWidthClass={maxWidthClass}
      />

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
              acting={acting}
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
