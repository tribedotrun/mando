import React, { useCallback, useRef, useState } from 'react';
import { Paperclip, X } from 'lucide-react';
import { toast } from 'sonner';
import type { ClarifierQuestion } from '#renderer/global/types';
import { useTaskClarify } from '#renderer/domains/captain/runtime/hooks';
import { useDraftRecord } from '#renderer/domains/captain/runtime/useDraft';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import {
  buildClarifyPayload,
  clarifyFingerprint,
  filledAnswerCount,
  getUnansweredQuestions,
} from '#renderer/domains/captain/service/clarifyHelpers';
import { clarifyResultToToast } from '#renderer/global/service/utils';
import { CardShell, StatusDot } from '#renderer/global/ui/CardShell';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';

export function ClarificationTab({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  const unanswered = getUnansweredQuestions(questions);
  const qFingerprint = clarifyFingerprint(unanswered);
  const [answers, setAnswers, clearAnswersDraft] = useDraftRecord(
    `mando:draft:clarify:${taskId}:${qFingerprint}`,
  );
  const [completed, setCompleted] = useState<string | null>(null);
  const filledCount = filledAnswerCount(unanswered, answers);
  const clarifyMut = useTaskClarify();

  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const fileRef = useRef<HTMLInputElement>(null);

  const handleSubmit = useCallback(async () => {
    const payload = buildClarifyPayload(unanswered, answers);
    if (payload.length === 0) return;

    try {
      const images = image ? [image] : undefined;
      const result = await clarifyMut.mutateAsync({
        id: taskId,
        mode: 'structured' as const,
        answers: payload,
        images,
      });
      const { variant, msg } = clarifyResultToToast(result.status);
      const fn = variant === 'success' ? toast.success : toast.info;
      fn(msg);
      clearAnswersDraft();
      removeImage();
      if (result.status !== 'clarifying') setCompleted(msg);
    } catch {
      // toast handled by mutation hook
    }
  }, [answers, unanswered, taskId, clearAnswersDraft, image, removeImage, clarifyMut]);

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
            disabled={clarifyMut.isPending}
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
          disabled={filledCount === 0 || clarifyMut.isPending}
          size="sm"
        >
          {clarifyMut.isPending ? 'Submitting...' : `Submit (${filledCount})`}
        </Button>
      </div>
    </div>
  );
}
