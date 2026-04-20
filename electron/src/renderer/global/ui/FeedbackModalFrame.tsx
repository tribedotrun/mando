import React from 'react';
import {
  Dialog,
  DialogContentPlain,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '#renderer/global/ui/dialog';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { Label } from '#renderer/global/ui/label';

export interface FeedbackModalFrameProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requireFeedback?: boolean;
  onCancel: () => void;
  feedback: string;
  setFeedback: React.Dispatch<React.SetStateAction<string>>;
  onSubmitClick: () => void;
  onPaste?: (event: React.ClipboardEvent) => void;
  attachmentPreview?: React.ReactNode;
  attachmentButton?: React.ReactNode;
}

export function FeedbackModalFrame({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  buttonLabel,
  pendingLabel,
  isPending,
  requireFeedback = true,
  onCancel,
  feedback,
  setFeedback,
  onSubmitClick,
  onPaste,
  attachmentPreview,
  attachmentButton,
}: FeedbackModalFrameProps): React.ReactElement {
  return (
    <Dialog open={true} onOpenChange={() => onCancel()}>
      <DialogContentPlain data-testid={testId}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle && (
            <DialogDescription className="truncate" title={subtitle}>
              {subtitle}
            </DialogDescription>
          )}
        </DialogHeader>

        {label && (
          <Label htmlFor="feedback-modal-text" className="text-muted-foreground">
            {label}
          </Label>
        )}
        <Textarea
          id="feedback-modal-text"
          className="min-h-[80px]"
          rows={3}
          placeholder={placeholder}
          value={feedback}
          onChange={(event) => setFeedback(event.target.value)}
          onPaste={onPaste}
          autoFocus
        />

        {attachmentPreview}

        <DialogFooter>
          {attachmentButton}
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={onSubmitClick}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
          >
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContentPlain>
    </Dialog>
  );
}
