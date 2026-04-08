import React, { useState, useCallback } from 'react';
import { Clock } from 'lucide-react';
import type { TaskItem, ClarifierQuestion, SessionSummary } from '#renderer/types';
import { answerClarification } from '#renderer/domains/captain/hooks/useApi';
import { useDraftRecord } from '#renderer/global/hooks/useDraft';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { toast } from 'sonner';
import log from '#renderer/logger';
import { clarifyResultToToast, fmtDuration, getErrorMessage, relativeTime } from '#renderer/utils';
import { PrIcon } from '#renderer/global/components/icons';
import { CardShell, StatusDot, Sep } from '#renderer/global/components/CardShell';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';

interface Props {
  item: TaskItem;
  sessions: SessionSummary[];
  /** Structured questions from latest clarify_question timeline event. */
  clarifierQuestions: ClarifierQuestion[] | null;
}

/* -- Variant renderers -- */

function StreamingCard({ item, sessions }: Pick<Props, 'item' | 'sessions'>) {
  const active = sessions.find((s) => s.status === 'running');
  const dur = active ? (active.duration_ms ?? 0) / 1000 : 0;
  return (
    <CardShell color="var(--review)">
      <StatusDot color="var(--review)" pulse />
      <span className="text-body font-medium text-foreground">Streaming</span>
      <Sep />
      <span className="text-caption text-muted-foreground">
        {item.worker ?? 'Worker'} &middot; {dur > 0 ? fmtDuration(dur) : 'starting'}
      </span>
    </CardShell>
  );
}

function QueuedCard() {
  return (
    <CardShell color="var(--text-4)">
      <Clock size={14} className="text-text-3" />
      <span className="text-body text-text-3">Queued</span>
    </CardShell>
  );
}

function CaptainReviewingCard({ label }: { label: string }) {
  return (
    <CardShell color="var(--review)">
      <StatusDot color="var(--review)" pulse />
      <span className="text-body font-medium text-foreground">{label}</span>
    </CardShell>
  );
}

function AwaitingReviewCard({ item }: { item: TaskItem }) {
  return (
    <CardShell color="var(--muted-foreground)">
      <StatusDot color="var(--muted-foreground)" />
      <span className="text-body font-medium text-foreground">Ready for review</span>
      {item.pr_number && (
        <>
          <Sep />
          <span className="text-caption text-muted-foreground">PR #{item.pr_number}</span>
        </>
      )}
    </CardShell>
  );
}

function EscalatedCard({ item }: { item: TaskItem }) {
  const [expanded, setExpanded] = useState(false);
  const preview = item.escalation_report?.slice(0, 120) ?? '';
  return (
    <CardShell color="var(--destructive)">
      <div className="flex w-full flex-col gap-1">
        <div className="flex items-center gap-2">
          <StatusDot color="var(--destructive)" />
          <span className="text-body font-medium text-foreground">Escalated</span>
        </div>
        {preview && (
          <div className="text-caption text-muted-foreground">
            &ldquo;{expanded ? item.escalation_report : preview}
            {!expanded && (item.escalation_report?.length ?? 0) > 120 ? '...' : ''}
            &rdquo;
            {(item.escalation_report?.length ?? 0) > 120 && (
              <Button
                variant="link"
                size="xs"
                className="ml-1 h-auto p-0"
                onClick={() => setExpanded((v) => !v)}
              >
                {expanded ? 'Less' : 'Full report'}
              </Button>
            )}
          </div>
        )}
      </div>
    </CardShell>
  );
}

function NeedsClarificationCard({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}) {
  const unanswered = questions.filter((q) => !q.self_answered);
  // Include question count + first question hash so stale drafts from a
  // different question set are automatically ignored on key change.
  const qFingerprint = `${unanswered.length}:${unanswered
    .map((q) => q.question)
    .join('|')
    .slice(0, 64)}`;
  const [answers, setAnswers, clearAnswersDraft] = useDraftRecord(
    `mando:draft:clarify:${taskId}:${qFingerprint}`,
  );
  const [pending, setPending] = useState(false);
  const [completed, setCompleted] = useState<string | null>(null);
  const taskFetch = useTaskStore((s) => s.fetch);
  const filledCount = unanswered.filter((_, i) => answers[i]?.trim()).length;

  const handleSubmit = useCallback(async () => {
    const payload = unanswered
      .map((q, i) => ({ question: q.question, answer: answers[i]?.trim() || '' }))
      .filter((a) => a.answer.length > 0);
    if (payload.length === 0) return;

    setPending(true);
    try {
      const result = await answerClarification(taskId, payload);
      void taskFetch();
      const { variant, msg } = clarifyResultToToast(result.status);
      const fn = variant === 'success' ? toast.success : toast.info;
      fn(msg);
      clearAnswersDraft();
      if (result.status !== 'clarifying') setCompleted(msg);
    } catch (err) {
      log.warn(`[StatusCard] clarification submit failed for task ${taskId}:`, err);
      toast.error(getErrorMessage(err, 'Failed to submit answers'));
    } finally {
      setPending(false);
    }
  }, [answers, unanswered, taskId, taskFetch, clearAnswersDraft]);

  if (completed) {
    return (
      <CardShell color="var(--muted-foreground)">
        <span className="text-body font-medium text-muted-foreground">{completed}</span>
      </CardShell>
    );
  }

  return (
    <div
      className="rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--needs-human) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--needs-human) 20%, transparent)',
      }}
    >
      <div className="mb-3 flex items-center gap-2">
        <StatusDot color="var(--needs-human)" />
        <span className="text-body font-medium text-foreground">Needs your input</span>
      </div>

      <div className="space-y-3">
        {unanswered.map((q, i) => (
          <div key={i}>
            <div className="mb-1 break-words text-body leading-snug text-foreground">
              <span className="text-text-3">{i + 1}.</span> {q.question}
            </div>
            <Textarea
              className="min-h-0 w-full resize-none bg-muted text-body leading-snug"
              rows={1}
              placeholder="Your answer..."
              value={answers[i] ?? ''}
              onChange={(e) => setAnswers({ ...answers, [i]: e.target.value })}
              disabled={pending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && e.metaKey && filledCount > 0) {
                  e.preventDefault();
                  void handleSubmit();
                }
              }}
            />
          </div>
        ))}
      </div>

      <div className="mt-3 flex items-center justify-between rounded-lg bg-muted px-3 py-2">
        <span className="text-caption text-text-3">
          {filledCount} of {unanswered.length} answered
        </span>
        <Button
          onClick={() => void handleSubmit()}
          disabled={filledCount === 0 || pending}
          size="sm"
        >
          {pending ? 'Submitting...' : `Submit (${filledCount})`}
        </Button>
      </div>
    </div>
  );
}

function FailedCard({ item }: { item: TaskItem }) {
  return (
    <CardShell color="var(--destructive)">
      <StatusDot color="var(--destructive)" />
      <span className="text-body font-medium text-foreground">
        {item.status === 'errored' ? 'Failed' : 'Rework'}
      </span>
      <Sep />
      <span className="text-caption text-muted-foreground">
        {item.intervention_count > 0 && `${item.intervention_count} interventions`}
      </span>
    </CardShell>
  );
}

function MergedCard({ item, sessions }: Pick<Props, 'item' | 'sessions'>) {
  return (
    <CardShell color="var(--text-4)">
      {item.status === 'merged' && <PrIcon state="merged" />}
      <span className="text-body text-text-3">
        {item.status === 'merged'
          ? 'Merged'
          : item.status === 'canceled'
            ? 'Canceled'
            : 'Completed'}
      </span>
      <Sep />
      <span className="text-caption text-text-3">
        {item.last_activity_at && relativeTime(item.last_activity_at)}
        {sessions.length > 0 && ` · ${sessions.length} sessions`}
      </span>
    </CardShell>
  );
}

/* -- Main export -- */

export function StatusCard({ item, sessions, clarifierQuestions }: Props): React.ReactElement {
  const s = item.status;

  if (s === 'needs-clarification' && clarifierQuestions && clarifierQuestions.length > 0) {
    return <NeedsClarificationCard taskId={item.id} questions={clarifierQuestions} />;
  }
  if (s === 'needs-clarification') {
    return (
      <CardShell color="var(--needs-human)">
        <StatusDot color="var(--needs-human)" />
        <span className="text-body font-medium text-foreground">Needs your input</span>
      </CardShell>
    );
  }

  if (s === 'in-progress' || s === 'clarifying')
    return <StreamingCard item={item} sessions={sessions} />;
  if (s === 'new' || s === 'queued') return <QueuedCard />;
  if (s === 'captain-reviewing') return <CaptainReviewingCard label="Captain reviewing" />;
  if (s === 'captain-merging') return <CaptainReviewingCard label="Captain merging" />;
  if (s === 'awaiting-review') return <AwaitingReviewCard item={item} />;
  if (s === 'escalated') return <EscalatedCard item={item} />;
  if (s === 'errored' || s === 'rework') return <FailedCard item={item} />;
  if (s === 'handed-off') {
    return (
      <CardShell color="var(--text-3)">
        <span className="text-body text-text-3">Handed off</span>
      </CardShell>
    );
  }

  // merged, completed-no-pr, canceled
  return <MergedCard item={item} sessions={sessions} />;
}
