import React, { useState } from 'react';
import { X } from 'lucide-react';
import type { ClarifierQuestion } from '#renderer/global/types';
import { useClarificationTab } from '#renderer/domains/captain/runtime/useClarificationTab';
import { CardShell, StatusDot } from '#renderer/global/ui/CardShell';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { TaskAttachmentButton } from '#renderer/domains/captain/ui/TaskComposerControls';

export function ClarificationTab({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  const {
    unanswered,
    answers,
    setAnswers,
    filledCount,
    clarifyMut,
    image,
    preview,
    setImageFile,
    removeImage,
    handleSubmit,
  } = useClarificationTab(taskId, questions);
  const [completed, setCompleted] = useState<string | null>(null);

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
              disabled={clarifyMut.isPending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && e.metaKey && filledCount > 0) {
                  e.preventDefault();
                  void handleSubmit().then((msg) => {
                    if (msg) setCompleted(msg);
                  });
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
          <TaskAttachmentButton
            onImageSelect={setImageFile}
            size="icon-xs"
            disabled={clarifyMut.isPending}
            className="text-muted-foreground"
          />
          <span className="text-caption text-text-3">
            {filledCount} of {unanswered.length} answered
          </span>
        </div>
        <Button
          onClick={() =>
            void handleSubmit().then((msg) => {
              if (msg) setCompleted(msg);
            })
          }
          disabled={filledCount === 0 || clarifyMut.isPending}
          size="sm"
        >
          {clarifyMut.isPending ? 'Submitting...' : `Submit (${filledCount})`}
        </Button>
      </div>
    </div>
  );
}
