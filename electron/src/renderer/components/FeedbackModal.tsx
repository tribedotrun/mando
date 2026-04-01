import React, { useState } from 'react';
import { useFocusTrap } from '#renderer/hooks/useFocusTrap';

interface FeedbackModalProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
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
  buttonLabel,
  pendingLabel,
  isPending,
  requireFeedback = true,
  onSubmit,
  onCancel,
}: FeedbackModalProps): React.ReactElement {
  const [feedback, setFeedback] = useState('');
  const { ref: dialogRef, handleKeyDown } = useFocusTrap(onCancel);

  return (
    <div
      data-testid={testId}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
      onKeyDown={handleKeyDown}
    >
      <div
        ref={dialogRef}
        className="w-[440px] max-w-[90vw] rounded-lg p-5"
        style={{ background: 'var(--color-surface-2)', border: '1px solid var(--color-border)' }}
      >
        <h3 className="text-subheading mb-1" style={{ color: 'var(--color-text-1)' }}>
          {title}
        </h3>
        {subtitle && (
          <p
            className="text-body mb-3 truncate"
            style={{ color: 'var(--color-text-2)' }}
            title={subtitle}
          >
            {subtitle}
          </p>
        )}

        {label && (
          <div className="text-label mb-1.5" style={{ color: 'var(--color-text-4)' }}>
            {label}
          </div>
        )}
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
            style={{ color: 'var(--color-text-2)', border: '1px solid var(--color-border)' }}
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit(feedback)}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
            className="rounded-md px-4 py-1.5 text-[13px] font-semibold disabled:opacity-50"
            style={{ background: 'var(--color-accent)', color: 'var(--color-bg)' }}
          >
            {isPending ? pendingLabel : buttonLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
