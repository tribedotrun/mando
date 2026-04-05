import React, { useState, useCallback } from 'react';
import type { ClarifierQuestion } from '#renderer/types';
import { answerClarification } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useToastStore } from '#renderer/stores/toastStore';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';

interface Props {
  taskId: number;
  questions: ClarifierQuestion[];
}

export function ClarificationSection({ taskId, questions }: Props): React.ReactElement {
  const unanswered = questions.filter((q) => !q.self_answered);
  const [answers, setAnswers] = useState<Record<number, string>>({});
  const [pending, setPending] = useState(false);
  const [completed, setCompleted] = useState<string | null>(null);
  const taskFetch = useTaskStore((s) => s.fetch);
  const toast = useToastStore.getState;

  const filledCount = unanswered.filter((_, i) => answers[i]?.trim()).length;

  const handleSubmit = useCallback(async () => {
    const payload = unanswered
      .map((q, i) => ({ question: q.question, answer: answers[i]?.trim() || '' }))
      .filter((a) => a.answer.length > 0);
    if (payload.length === 0) return;

    setPending(true);
    try {
      const result = await answerClarification(taskId, payload);
      taskFetch();
      const msgs: Record<string, [variant: 'success' | 'info', msg: string]> = {
        ready: ['success', 'Clarified — task queued'],
        clarifying: ['info', 'Still needs more info'],
        escalate: ['info', 'Escalated to captain review'],
      };
      const [variant, msg] = msgs[result.status] ?? ['success', 'Answer saved'];
      toast().add(variant, msg);
      if (result.status !== 'clarifying') setCompleted(msg);
      else setAnswers({});
    } catch (err) {
      log.warn(`[ClarificationSection] submit failed for task ${taskId}:`, err);
      toast().add('error', getErrorMessage(err, 'Failed to submit answers'));
    } finally {
      setPending(false);
    }
  }, [answers, unanswered, taskId, taskFetch, toast]);

  if (completed) {
    return (
      <div className="mb-5">
        <div
          className="rounded-lg px-4 py-3 text-[12px] font-medium"
          style={{ color: 'var(--color-success)', background: 'var(--color-surface-2)' }}
        >
          {completed}
        </div>
      </div>
    );
  }

  return (
    <div className="mb-5">
      {/* Questions with input fields */}
      <div className="space-y-4">
        {unanswered.map((q, i) => (
          <div key={i}>
            <div
              className="mb-1.5 break-words text-[13px] leading-snug"
              style={{ color: 'var(--color-text-1)' }}
            >
              <span style={{ color: 'var(--color-text-3)' }}>{i + 1}.</span> {q.question}
            </div>
            <textarea
              className="w-full resize-none rounded-md bg-transparent px-3 py-2 text-[13px] leading-snug focus:outline-none"
              style={{
                color: 'var(--color-text-1)',
                border: '1px solid var(--color-border-subtle)',
                background: 'var(--color-surface-2)',
              }}
              rows={1}
              placeholder="Your answer..."
              value={answers[i] ?? ''}
              onChange={(e) => setAnswers((prev) => ({ ...prev, [i]: e.target.value }))}
              disabled={pending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && e.metaKey && filledCount > 0) {
                  e.preventDefault();
                  handleSubmit();
                }
              }}
            />
          </div>
        ))}
      </div>

      {/* Submit bar */}
      <div
        className="mt-4 flex items-center justify-between rounded-lg px-4 py-2.5"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
        }}
      >
        <span className="text-[12px]" style={{ color: 'var(--color-text-3)' }}>
          {filledCount} of {unanswered.length} answered
        </span>
        <button
          onClick={handleSubmit}
          disabled={filledCount === 0 || pending}
          className="rounded-md px-4 py-1.5 text-[12px] font-medium disabled:opacity-40"
          style={{
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            cursor: filledCount === 0 || pending ? 'default' : 'pointer',
          }}
        >
          {pending ? 'Submitting...' : 'Submit Answers'}
        </button>
      </div>
    </div>
  );
}
