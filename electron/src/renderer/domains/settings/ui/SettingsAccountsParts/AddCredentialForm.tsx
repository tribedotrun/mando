import React from 'react';
import { Input } from '#renderer/global/ui/primitives/input';
import { Label } from '#renderer/global/ui/primitives/label';
import { Button } from '#renderer/global/ui/primitives/button';
import { useAddCredentialForm } from '#renderer/domains/settings/runtime/useAddCredentialForm';

interface AddCredentialFormProps {
  onClose: () => void;
}

export function AddCredentialForm({ onClose }: AddCredentialFormProps): React.ReactElement {
  const form = useAddCredentialForm(onClose);

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground/70">
        Run <code className="rounded bg-muted px-1 py-0.5">claude setup-token</code> in a terminal
        to generate a token from another account.
      </p>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Label</Label>
        <Input
          data-testid="setup-label-input"
          type="text"
          value={form.fields.label}
          onChange={(e) => form.fields.setLabel(e.target.value)}
          placeholder="e.g. team-account-2"
          autoFocus
        />
      </div>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Setup Token</Label>
        <Input
          data-testid="setup-token-input"
          type="text"
          value={form.fields.token}
          onChange={(e) => form.fields.setToken(e.target.value)}
          placeholder="Paste setup token..."
        />
      </div>
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!form.fields.token.trim() || !form.fields.label.trim() || form.state.pending}
          onClick={form.actions.handleAdd}
        >
          {form.state.pending ? 'Adding...' : 'Add Credential'}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={form.actions.handleClose}
          disabled={form.state.pending}
        >
          Cancel
        </Button>
      </div>
    </div>
  );
}
