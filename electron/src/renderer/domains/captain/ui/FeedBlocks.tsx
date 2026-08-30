import React from 'react';
import {
  shouldSuppressTimelineEvent,
  type RenderableFeedItem,
} from '#renderer/domains/captain/service/feedHelpers';
import { getUnansweredQuestions } from '#renderer/domains/captain/service/clarifyHelpers';
import { MessageBlock } from '#renderer/domains/captain/ui/MessageBlock';
import { EvidenceBlock, WorkSummaryBlock } from '#renderer/domains/captain/ui/ArtifactBlocks';
import {
  ActiveClarificationBlock,
  ClarificationSummaryBlock,
  ClarifierAnsweredBlock,
  EscalationBlock,
  TimelineBlock,
} from '#renderer/domains/captain/ui/FeedBlocksParts';
import { ClarifierFailedRow } from '#renderer/domains/captain/ui/ClarifierFailedCard';
import type { TaskItem, AskHistoryEntry } from '#renderer/global/types';

export function FeedBlocks({
  item,
  task,
  isLatestClarify,
  isArtifactExpanded,
}: {
  item: RenderableFeedItem;
  task: TaskItem;
  isLatestClarify: (timestamp: string) => boolean;
  isArtifactExpanded: (id: number) => boolean;
}): React.ReactElement | null {
  switch (item.type) {
    case 'timeline': {
      const event = item.data;
      const payload = event.data;
      if (payload.event_type === 'escalated') {
        return <EscalationBlock event={event} report={task.escalation_report} />;
      }
      if (payload.event_type === 'clarify_question') {
        const questions = payload.questions ?? [];
        const unanswered = getUnansweredQuestions(questions);
        return task.status === 'needs-clarification' &&
          isLatestClarify(event.timestamp) &&
          unanswered.length > 0 ? (
          <ActiveClarificationBlock taskId={task.id} questions={unanswered} />
        ) : (
          <ClarificationSummaryBlock event={event} questions={questions} />
        );
      }
      if (payload.event_type === 'clarifier_failed') {
        return <ClarifierFailedRow taskStatus={task.status} event={event} payload={payload} />;
      }
      if (payload.event_type === 'clarifier_completed_no_pr') {
        return task.status === 'completed-no-pr' ? (
          <ClarifierAnsweredBlock event={event} taskContext={task.context ?? ''} />
        ) : (
          <TimelineBlock event={event} />
        );
      }
      if (shouldSuppressTimelineEvent(payload.event_type)) return null;
      return <TimelineBlock event={event} />;
    }
    case 'artifact': {
      const artifact = item.data;
      const expanded = isArtifactExpanded(artifact.id);
      if (artifact.artifact_type === 'evidence')
        return <EvidenceBlock artifacts={[artifact]} initialExpanded={expanded} />;
      if (artifact.artifact_type === 'work_summary')
        return <WorkSummaryBlock artifact={artifact} initialExpanded={expanded} />;
      return null;
    }
    case 'evidence-group': {
      const expanded = item.artifacts.some((a) => isArtifactExpanded(a.id));
      return <EvidenceBlock artifacts={item.artifacts} initialExpanded={expanded} />;
    }
    case 'message':
      return <MessageBlock entry={item.data as AskHistoryEntry} />;
    default:
      return null;
  }
}
