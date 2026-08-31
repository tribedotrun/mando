import React, { useCallback, useMemo, useRef } from 'react';
import { useTaskFeed, useTaskReopen, useTaskRework } from '#renderer/domains/captain/runtime/hooks';
import { useExpandedArtifactIds } from '#renderer/domains/captain/runtime/useExpandedArtifactIds';
import { FeedBlocks } from '#renderer/domains/captain/ui/FeedBlocks';
import { ReopenReworkComposer } from '#renderer/domains/captain/ui/ReopenReworkComposer';
import type { ReopenReworkIntent } from '#renderer/domains/captain/runtime/useReopenReworkComposer';
import {
  groupEvidenceArtifacts,
  latestClarifyTimestamp,
} from '#renderer/domains/captain/service/feedHelpers';
import type { TaskItem } from '#renderer/global/types';
import { Clock } from 'lucide-react';

interface TaskFeedViewProps {
  item: TaskItem;
}

export function TaskFeedView({ item }: TaskFeedViewProps): React.ReactElement {
  const feedEndRef = useRef<HTMLDivElement>(null);
  const { data: feedData } = useTaskFeed(item.id);
  const reopenMutation = useTaskReopen();
  const reworkMutation = useTaskRework();

  const feedItems = feedData?.feed ?? [];

  const latestClarifyTs = useMemo(() => latestClarifyTimestamp(feedItems), [feedItems]);
  const isLatestClarify = useCallback((ts: string) => ts === latestClarifyTs, [latestClarifyTs]);

  // useExpandedArtifactIds reads the un-grouped feed so its
  // "latest screenshot/recording" tracking still sees every individual
  // evidence artifact. Grouping happens only at the render layer below.
  const isArtifactExpanded = useExpandedArtifactIds(feedItems);
  const renderableItems = useMemo(() => groupEvidenceArtifacts(feedItems), [feedItems]);

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
    (message: string, intent: ReopenReworkIntent) => {
      if (intent === 'reopen') {
        reopenMutation.mutate({ id: item.id, feedback: message });
      } else {
        reworkMutation.mutate({ id: item.id, feedback: message });
      }
    },
    [item.id, reopenMutation, reworkMutation],
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
            {renderableItems.map((entry, i) => {
              // Earliest artifact id keeps the React key stable when a new
              // evidence artifact is appended to the group — local expand
              // state in EvidenceBlock survives the re-render.
              const key =
                entry.type === 'evidence-group'
                  ? `evidence-group-${entry.artifacts[0]?.id ?? i}`
                  : `${entry.type}-${entry.timestamp}-${i}`;
              return (
                <FeedBlocks
                  key={key}
                  item={entry}
                  task={item}
                  isLatestClarify={isLatestClarify}
                  isArtifactExpanded={isArtifactExpanded}
                />
              );
            })}
            <div ref={feedEndCallbackRef} />
          </div>
        )}
      </div>

      <ReopenReworkComposer
        item={item}
        onSend={handleSend}
        isPending={reopenMutation.isPending || reworkMutation.isPending}
      />
    </div>
  );
}
