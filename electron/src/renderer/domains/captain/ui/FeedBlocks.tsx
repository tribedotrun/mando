import React from 'react';
import { shouldSuppressTimelineEvent } from '#renderer/domains/captain/service/feedHelpers';
import {
  CompletedPlanBlock,
  ReadyPlanBlock,
} from '#renderer/domains/captain/ui/PlanCompletedBlock';
import { MessageBlock } from '#renderer/domains/captain/ui/MessageBlock';
import { EvidenceBlock, WorkSummaryBlock } from '#renderer/domains/captain/ui/ArtifactBlocks';
import {
  ActiveClarificationBlock,
  ClarificationSummaryBlock,
  EscalationBlock,
  TimelineBlock,
} from '#renderer/domains/captain/ui/FeedBlocksParts';
import { ClarifierFailedRow } from '#renderer/domains/captain/ui/ClarifierFailedCard';
import type { TaskItem, FeedItem, TaskArtifact, AskHistoryEntry } from '#renderer/global/types';

export function FeedBlock({
  item,
  task,
  isLatestClarify,
  isArtifactExpanded,
}: {
  item: FeedItem;
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
        return task.status === 'needs-clarification' &&
          isLatestClarify(event.timestamp) &&
          questions.length > 0 ? (
          <ActiveClarificationBlock taskId={task.id} questions={questions} />
        ) : (
          <ClarificationSummaryBlock event={event} questions={questions} />
        );
      }
      if (payload.event_type === 'clarifier_failed') {
        return <ClarifierFailedRow taskId={task.id} event={event} payload={payload} />;
      }
      if (payload.event_type === 'plan_completed') {
        return task.status === 'plan-ready' ? (
          <ReadyPlanBlock event={event} taskId={task.id} taskContext={task.context ?? ''} />
        ) : (
          <CompletedPlanBlock event={event} />
        );
      }
      if (shouldSuppressTimelineEvent(payload.event_type)) return null;
      return <TimelineBlock event={event} />;
    }
    case 'artifact': {
      const artifact = item.data as TaskArtifact;
      const expanded = isArtifactExpanded(artifact.id);
      if (artifact.artifact_type === 'evidence')
        return <EvidenceBlock artifact={artifact} initialExpanded={expanded} />;
      if (artifact.artifact_type === 'work_summary')
        return <WorkSummaryBlock artifact={artifact} initialExpanded={expanded} />;
      return null;
    }
    case 'message':
      return <MessageBlock entry={item.data as AskHistoryEntry} />;
    default:
      return null;
  }
}
