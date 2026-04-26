import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '#renderer/global/ui/primitives/dialog';
import { Button } from '#renderer/global/ui/primitives/button';

interface ActivateCodexConfirmModalProps {
  open: boolean;
  label: string;
  onClose: () => void;
}

export function ActivateCodexConfirmModal({
  open,
  label,
  onClose,
}: ActivateCodexConfirmModalProps): React.ReactElement {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent data-testid="codex-activate-confirm">
        <DialogHeader>
          <DialogTitle>Switched to {label}</DialogTitle>
          <DialogDescription>
            Your <code>~/.codex/auth.json</code> now points at this account. Conversation history in{' '}
            <code>~/.codex/sessions</code> is unchanged.
          </DialogDescription>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">
          Codex caches the access token at process start, so any running clients need to be
          relaunched to pick up the new account:
        </p>
        <ul className="ml-5 list-disc text-sm text-muted-foreground">
          <li>Codex CLI sessions (open a fresh terminal)</li>
          <li>Codex desktop app (Quit and reopen)</li>
          <li>VS Code / JetBrains Codex extensions</li>
        </ul>
        <DialogFooter>
          <Button onClick={onClose}>Got it</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
