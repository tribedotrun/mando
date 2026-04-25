import React, { useState } from 'react';
import { X } from 'lucide-react';
import type { ClarifierQuestion } from '#renderer/global/types';
import { useClarificationTab } from '#renderer/domains/captain/runtime/useClarificationTab';
import { CardFrame, StatusDot } from '#renderer/domains/captain/ui/CardFrame';
import { Button } from '#renderer/global/ui/primitives/button';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { TaskAttachmentButton } from '#renderer/domains/captain/ui/TaskAttachmentButton';

export function ClarificationTab({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  const form = useClarificationTab(taskId, questions);
  const [completed, setCompleted] = useState<string | null>(null);

  const submitAnswers = async (): Promise<void> => {
    const msg = await form.actions.handleSubmit();
    if (msg) setCompleted(msg);
  };

  if (completed) {
    return (
      <CardFrame color="var(--muted-foreground)">
        <span className="text-body font-medium text-muted-foreground">{completed}</span>
      </CardFrame>
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
        {form.questions.unanswered.map((q, i) => (
          <div key={i}>
            <div className="mb-1 break-words text-body leading-snug text-foreground">
              <span className="text-text-3">{i + 1}.</span> {q.question}
            </div>
            <Textarea
              data-testid="clarifier-answer"
              data-answer-index={i}
              className="min-h-0 w-full resize-none bg-muted text-body leading-snug"
              rows={1}
              placeholder="Your answer..."
              value={form.questions.answers[i] ?? ''}
              onChange={(e) =>
                form.questions.setAnswers({ ...form.questions.answers, [i]: e.target.value })
              }
              disabled={form.mutation.clarifyMut.isPending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && e.metaKey && form.questions.filledCount > 0) {
                  e.preventDefault();
                  void submitAnswers();
                }
              }}
            />
          </div>
        ))}
      </div>

      {form.image.preview && form.image.image && (
        <div className="mt-2 flex items-center gap-2 rounded-lg bg-muted px-3 py-2">
          <img
            src={form.image.preview}
            alt={form.image.image.name}
            className="h-10 w-10 rounded-md object-cover"
          />
          <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
            {form.image.image.name}
          </span>
          <Button variant="ghost" size="icon-xs" onClick={form.image.removeImage}>
            <X size={12} />
          </Button>
        </div>
      )}

      <div className="mt-3 flex items-center justify-between rounded-lg bg-muted px-3 py-2">
        <div className="flex items-center gap-2">
          <TaskAttachmentButton
            onImageSelect={form.image.setImageFile}
            size="icon-xs"
            disabled={form.mutation.clarifyMut.isPending}
            className="text-muted-foreground"
          />
          <span className="text-caption text-text-3">
            {form.questions.filledCount} of {form.questions.unanswered.length} answered
          </span>
        </div>
        <Button
          data-testid="clarifier-submit"
          onClick={() => void submitAnswers()}
          disabled={form.questions.filledCount === 0 || form.mutation.clarifyMut.isPending}
          size="sm"
        >
          {form.mutation.clarifyMut.isPending
            ? 'Submitting...'
            : `Submit (${form.questions.filledCount})`}
        </Button>
      </div>
    </div>
  );
}
