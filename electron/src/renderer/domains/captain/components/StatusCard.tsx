import React, { useState, useCallback, useRef } from 'react';
import { Paperclip, X } from 'lucide-react';
import type { TaskItem, ClarifierQuestion, SessionSummary } from '#renderer/types';
import { answerClarification } from '#renderer/domains/captain/hooks/useApi';
import { useDraftRecord } from '#renderer/global/hooks/useDraft';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { toast } from 'sonner';
import log from '#renderer/logger';
import { clarifyResultToToast, fmtDuration, getErrorMessage } from '#renderer/utils';
import { CardShell, StatusDot } from '#renderer/global/components/CardShell';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';

/* -- Clarification card (used as tab content) -- */

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
  const filledCount = unanswered.filter((_, i) => answers[i]?.trim()).length;

  // Image state
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const previewRef = useRef(preview);
  previewRef.current = preview;

  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  const setImageFile = useCallback((file: File) => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    const url = URL.createObjectURL(file);
    setImage(file);
    setPreview(url);
    previewRef.current = url;
  }, []);

  const removeImage = useCallback(() => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(null);
    setPreview(null);
    previewRef.current = null;
  }, []);

  const handleSubmit = useCallback(async () => {
    const payload = unanswered
      .map((q, i) => ({ question: q.question, answer: answers[i]?.trim() || '' }))
      .filter((a) => a.answer.length > 0);
    if (payload.length === 0) return;

    setPending(true);
    try {
      const images = image ? [image] : undefined;
      const result = await answerClarification(taskId, payload, images);
      // SSE handles cache update
      const { variant, msg } = clarifyResultToToast(result.status);
      const fn = variant === 'success' ? toast.success : toast.info;
      fn(msg);
      clearAnswersDraft();
      removeImage();
      if (result.status !== 'clarifying') setCompleted(msg);
    } catch (err) {
      log.warn(`[StatusCard] clarification submit failed for task ${taskId}:`, err);
      toast.error(getErrorMessage(err, 'Failed to submit answers'));
    } finally {
      setPending(false);
    }
  }, [answers, unanswered, taskId, clearAnswersDraft, image, removeImage]);

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

      {preview && image && (
        <div className="mt-2 flex items-center gap-2 rounded-lg bg-muted px-3 py-2">
          <img src={preview} alt={image.name} className="h-10 w-10 rounded-md object-cover" />
          <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
            {image.name}
          </span>
          <Button variant="ghost" size="icon-xs" onClick={removeImage}>
            <X size={12} />
          </Button>
        </div>
      )}

      <div className="mt-3 flex items-center justify-between rounded-lg bg-muted px-3 py-2">
        <div className="flex items-center gap-2">
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) setImageFile(file);
              e.target.value = '';
            }}
          />
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => fileRef.current?.click()}
            disabled={pending}
            aria-label="Attach image"
            className="text-muted-foreground"
          >
            <Paperclip size={14} />
          </Button>
          <span className="text-caption text-text-3">
            {filledCount} of {unanswered.length} answered
          </span>
        </div>
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

/* -- Inline header badge (compact, single-line) -- */

interface HeaderBadgeProps {
  item: TaskItem;
  sessions: SessionSummary[];
}

export function HeaderStatusBadge({ item, sessions }: HeaderBadgeProps): React.ReactElement {
  const s = item.status;

  if (s === 'in-progress' || s === 'clarifying') {
    const active = sessions.find((ss) => ss.status === 'running');
    const dur = active ? (active.duration_ms ?? 0) / 1000 : 0;
    return (
      <Badge color="var(--success)" pulse>
        Streaming{dur > 0 ? ` ${fmtDuration(dur)}` : ''}
      </Badge>
    );
  }
  if (s === 'new' || s === 'queued') return <Badge color="var(--text-4)">Queued</Badge>;
  if (s === 'captain-reviewing')
    return (
      <Badge color="var(--success)" pulse>
        Reviewing
      </Badge>
    );
  if (s === 'captain-merging')
    return (
      <Badge color="var(--success)" pulse>
        Merging
      </Badge>
    );
  if (s === 'awaiting-review') return <Badge color="var(--review)">Ready for review</Badge>;
  if (s === 'escalated') return <Badge color="var(--destructive)">Escalated</Badge>;
  if (s === 'needs-clarification') return <Badge color="var(--needs-human)">Needs input</Badge>;
  if (s === 'errored') return <Badge color="var(--destructive)">Failed</Badge>;
  if (s === 'rework') return <Badge color="var(--destructive)">Rework</Badge>;
  if (s === 'handed-off') return <Badge color="var(--text-3)">Handed off</Badge>;
  if (s === 'merged') return <Badge color="var(--text-4)">Merged</Badge>;
  if (s === 'canceled') return <Badge color="var(--text-4)">Canceled</Badge>;
  return <Badge color="var(--text-4)">Completed</Badge>;
}

function Badge({
  color,
  pulse,
  children,
}: {
  color: string;
  pulse?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <span
      className="flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-0.5"
      style={{
        background: `color-mix(in srgb, ${color} 10%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 25%, transparent)`,
      }}
    >
      <StatusDot color={color} pulse={pulse} size="sm" />
      <span className="text-caption font-medium" style={{ color }}>
        {children}
      </span>
    </span>
  );
}

/* -- Tab content for escalated report -- */

export function EscalatedReportTab({ item }: { item: TaskItem }): React.ReactElement {
  return (
    <div className="space-y-3">
      {item.escalation_report ? (
        <PrMarkdown text={item.escalation_report} />
      ) : (
        <div className="text-body text-muted-foreground">No escalation report available.</div>
      )}
    </div>
  );
}

/* -- Tab content for clarification (re-export for use in tabs) -- */

export { NeedsClarificationCard as ClarificationTab };
