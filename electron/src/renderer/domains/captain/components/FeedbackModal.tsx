import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '#renderer/global/components/Dialog';

interface FeedbackModalProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  initialValue?: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requireFeedback?: boolean;
  onSubmit: (feedback: string) => void;
  onCancel: () => void;
}

export function FeedbackModal({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  initialValue,
  buttonLabel,
  pendingLabel,
  isPending,
  requireFeedback = true,
  onSubmit,
  onCancel,
}: FeedbackModalProps): React.ReactElement {
  const [feedback, setFeedback] = useState(initialValue ?? '');

  return (
    <Dialog open={true} onOpenChange={() => onCancel()}>
      <DialogContent data-testid={testId}>
        <DialogTitle className="mb-1">{title}</DialogTitle>
        {subtitle && (
          <DialogDescription className="truncate" title={subtitle}>
            {subtitle}
          </DialogDescription>
        )}

        {label && <div className="text-label mb-1.5 text-text-4">{label}</div>}
        <textarea
          className="mb-3 w-full rounded-md px-3 py-2 text-[13px] focus:outline-none"
          style={{
            background: 'var(--color-surface-1)',
            color: 'var(--color-text-1)',
            border: '1px solid var(--color-border-subtle)',
          }}
          rows={3}
          placeholder={placeholder}
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          autoFocus
        />
        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md px-3 py-1.5 text-[13px]"
            style={{
              background: 'transparent',
              color: 'var(--color-text-2)',
              border: '1px solid var(--color-border)',
            }}
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit(feedback)}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
            className="rounded-md px-4 py-1.5 text-[13px] font-semibold disabled:opacity-50"
            style={{
              background: 'var(--color-accent)',
              color: 'var(--color-bg)',
              fontWeight: 600,
            }}
          >
            {isPending ? pendingLabel : buttonLabel}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
