import React from 'react';
import {
  Dialog,
  DialogContentPlain,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '#renderer/global/ui/primitives/dialog';
import { Button } from '#renderer/global/ui/primitives/button';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { Label } from '#renderer/global/ui/primitives/label';

export interface PromptModalFrameProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requirePrompt?: boolean;
  onCancel: () => void;
  prompt: string;
  setPrompt: (value: string) => void;
  onSubmitClick: () => void;
  onPaste?: (event: React.ClipboardEvent) => void;
  attachmentPreview?: React.ReactNode;
  attachmentButton?: React.ReactNode;
}

export function PromptModalFrame({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  buttonLabel,
  pendingLabel,
  isPending,
  requirePrompt = true,
  onCancel,
  prompt,
  setPrompt,
  onSubmitClick,
  onPaste,
  attachmentPreview,
  attachmentButton,
}: PromptModalFrameProps): React.ReactElement {
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
          <Label htmlFor="prompt-modal-text" className="text-muted-foreground">
            {label}
          </Label>
        )}
        <Textarea
          id="prompt-modal-text"
          className="min-h-[80px]"
          rows={3}
          placeholder={placeholder}
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          onPaste={onPaste}
          autoFocus
        />

        {attachmentPreview}

        <DialogFooter>
          {attachmentButton}
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={onSubmitClick} disabled={(requirePrompt && !prompt.trim()) || isPending}>
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContentPlain>
    </Dialog>
  );
}
