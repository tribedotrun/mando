import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '#renderer/components/ui/dialog';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';
import { Label } from '#renderer/components/ui/label';

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
      <DialogContent data-testid={testId} showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle && (
            <DialogDescription className="truncate" title={subtitle}>
              {subtitle}
            </DialogDescription>
          )}
        </DialogHeader>

        {label && <Label className="text-muted-foreground">{label}</Label>}
        <Textarea
          className="min-h-[80px]"
          rows={3}
          placeholder={placeholder}
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          autoFocus
        />

        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={() => onSubmit(feedback)}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
          >
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
