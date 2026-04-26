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
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import log from '#renderer/global/service/logger';

interface PromptModalProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  initialValue?: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requirePrompt?: boolean;
  draftKey: string;
  onCancel: () => void;
  /**
   * Resolved => clear draft. Rejected => keep draft so the user can retry
   * without retyping. Sync/void return clears eagerly.
   */
  onSubmit: (prompt: string) => Promise<void> | void;
}

export function PromptModal({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  initialValue,
  buttonLabel,
  pendingLabel,
  isPending,
  requirePrompt = true,
  draftKey,
  onCancel,
  onSubmit,
}: PromptModalProps): React.ReactElement {
  const {
    text: prompt,
    setText: setPrompt,
    clearDraftStorage,
  } = useTextImageDraft(draftKey, { initialText: initialValue });

  const handleSubmit = async () => {
    const result = onSubmit(prompt);
    if (result && typeof (result as Promise<void>).then === 'function') {
      try {
        await result;
        clearDraftStorage();
      } catch (err) {
        log.warn('[PromptModal] submit rejected; preserving draft for retry', { err });
      }
      return;
    }
    clearDraftStorage();
  };

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
          autoFocus
        />

        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              void handleSubmit();
            }}
            disabled={(requirePrompt && !prompt.trim()) || isPending}
          >
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContentPlain>
    </Dialog>
  );
}
