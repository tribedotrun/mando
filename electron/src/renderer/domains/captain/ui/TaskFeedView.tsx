import React, { useCallback, useMemo, useRef } from 'react';
import { useTaskFeed, useTaskAdvisor } from '#renderer/domains/captain/runtime/hooks';
import { useExpandedArtifactIds } from '#renderer/domains/captain/runtime/useExpandedArtifactIds';
import { FeedBlocks } from '#renderer/domains/captain/ui/FeedBlocks';
import { AdvisorInputBar } from '#renderer/domains/captain/ui/AdvisorInputBar';
import { latestClarifyTimestamp } from '#renderer/domains/captain/service/feedHelpers';
import type { TaskItem } from '#renderer/global/types';
import { Clock } from 'lucide-react';

interface TaskFeedViewProps {
  item: TaskItem;
}

export function TaskFeedView({ item }: TaskFeedViewProps): React.ReactElement {
  const feedEndRef = useRef<HTMLDivElement>(null);
  const { data: feedData } = useTaskFeed(item.id);
  const advisorMutation = useTaskAdvisor();

  const feedItems = feedData?.feed ?? [];

  const latestClarifyTs = useMemo(() => latestClarifyTimestamp(feedItems), [feedItems]);
  const isLatestClarify = useCallback((ts: string) => ts === latestClarifyTs, [latestClarifyTs]);

  const isArtifactExpanded = useExpandedArtifactIds(feedItems);

  const prevCountRef = useRef(0);
  const feedEndCallbackRef = useCallback(
    (node: HTMLDivElement | null) => {
      feedEndRef.current = node;
      if (node && feedItems.length > 0 && feedItems.length !== prevCountRef.current) {
        const isInitial = prevCountRef.current === 0;
        node.scrollIntoView({ behavior: isInitial ? 'instant' : 'smooth' });
      }
      prevCountRef.current = feedItems.length;
    },
    [feedItems.length],
  );

  const handleSend = useCallback(
    (message: string, intent: string) => {
      advisorMutation.mutate({ id: item.id, message, intent });
    },
    [advisorMutation, item.id],
  );

  return (
    <div className="flex h-full flex-col">
      <div className="scrollbar-on-hover min-h-0 flex-1 overflow-y-auto">
        {feedItems.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center text-text-3">
              <Clock size={32} className="mx-auto mb-2 opacity-50" />
              <p className="text-body">Waiting for activity...</p>
            </div>
          </div>
        ) : (
          <div className="pt-2">
            {feedItems.map((entry, i) => (
              <FeedBlocks
                key={`${entry.type}-${entry.timestamp}-${i}`}
                item={entry}
                task={item}
                isLatestClarify={isLatestClarify}
                isArtifactExpanded={isArtifactExpanded}
              />
            ))}
            <div ref={feedEndCallbackRef} />
          </div>
        )}
      </div>

      <AdvisorInputBar item={item} onSend={handleSend} isPending={advisorMutation.isPending} />
    </div>
  );
}
