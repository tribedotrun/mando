import React from 'react';
import {
  EVENT_ICON_MAP,
  confidenceIconOverride,
  confidencePreview,
  formatEventTime,
  getNudgeReason,
  shouldSuppressTimelineEvent,
} from '#renderer/domains/captain/service/feedHelpers';
import {
  CompletedPlanBlock,
  ReadyPlanBlock,
} from '#renderer/domains/captain/ui/PlanCompletedBlock';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { MessageBlock } from '#renderer/domains/captain/ui/MessageBlock';
import { EvidenceBlock, WorkSummaryBlock } from '#renderer/domains/captain/ui/ArtifactBlocks';
import { ClarificationTab } from '#renderer/domains/captain/ui/ClarificationTab';
import type {
  TaskItem,
  FeedItem,
  TimelineEvent,
  TaskArtifact,
  AskHistoryEntry,
  ClarifierQuestion,
} from '#renderer/global/types';
import { MessageSquare, AlertTriangle } from 'lucide-react';
import { StatusIcon } from '#renderer/global/ui/StatusIndicator';

function TimelineBlock({ event }: { event: TimelineEvent }) {
  const iconStatus = confidenceIconOverride(event) ?? EVENT_ICON_MAP[event.event_type] ?? 'queued';
  const time = formatEventTime(event.timestamp);
  const nudgeReason = getNudgeReason(event);
  const triageDetail = confidencePreview(event);

  return (
    <div className="flex items-start gap-3 px-3 py-2">
      <div className="mt-0.5 flex-shrink-0">
        <StatusIcon status={iconStatus} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="text-caption text-text-2">{time}</span>
          <span className="max-w-[120px] truncate text-caption font-medium text-text-2">
            {event.actor}
          </span>
        </div>
        <p className="break-words text-body text-text-1">{event.summary}</p>
        {nudgeReason ? (
          <p className="mt-0.5 text-caption text-text-3 [overflow-wrap:anywhere]">
            Reason: {nudgeReason}
          </p>
        ) : null}
        {triageDetail ? (
          <p className="mt-0.5 text-caption text-text-3 [overflow-wrap:anywhere]">{triageDetail}</p>
        ) : null}
      </div>
    </div>
  );
}

function EscalationBlock({ event, report }: { event: TimelineEvent; report?: string | null }) {
  const time = formatEventTime(event.timestamp);

  return (
    <div
      className="mx-3 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--destructive) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--destructive) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <AlertTriangle size={14} className="text-destructive" />
        <span className="text-body font-medium text-destructive">Escalated</span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {report ? (
        <div className="text-body text-text-1">
          <PrMarkdown text={report} />
        </div>
      ) : (
        <p className="break-words text-body text-text-1">{event.summary}</p>
      )}
    </div>
  );
}

function ActiveClarificationBlock({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  return (
    <div className="mx-3 my-2">
      <ClarificationTab taskId={taskId} questions={questions} />
    </div>
  );
}

function ClarificationSummaryBlock({
  event,
  questions,
}: {
  event: TimelineEvent;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  const time = formatEventTime(event.timestamp);

  return (
    <div
      className="mx-3 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--needs-human) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--needs-human) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <MessageSquare size={14} style={{ color: 'var(--needs-human)' }} />
        <span className="text-body font-medium" style={{ color: 'var(--needs-human)' }}>
          Clarification requested
        </span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {questions.map((question, index) => (
        <div key={index} className="mb-1 text-body text-text-2">
          <span className="text-text-3">{index + 1}.</span> {question.question}
          {question.self_answered && (
            <span className="ml-1 text-caption text-text-3">(auto-resolved)</span>
          )}
        </div>
      ))}
    </div>
  );
}

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
      const event = item.data as TimelineEvent;
      if (event.event_type === 'escalated') {
        return <EscalationBlock event={event} report={task.escalation_report} />;
      }
      if (event.event_type === 'clarify_question') {
        const questions = (event.data?.questions as ClarifierQuestion[]) ?? [];
        return task.status === 'needs-clarification' &&
          isLatestClarify(event.timestamp) &&
          questions.length > 0 ? (
          <ActiveClarificationBlock taskId={task.id} questions={questions} />
        ) : (
          <ClarificationSummaryBlock event={event} questions={questions} />
        );
      }
      if (event.event_type === 'plan_completed') {
        return task.status === 'plan-ready' ? (
          <ReadyPlanBlock event={event} taskId={task.id} taskContext={task.context ?? ''} />
        ) : (
          <CompletedPlanBlock event={event} />
        );
      }
      if (shouldSuppressTimelineEvent(event.event_type)) return null;
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
