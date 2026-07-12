import React from 'react';
import { Input } from '#renderer/global/ui/primitives/input';
import { Label } from '#renderer/global/ui/primitives/label';
import { Button } from '#renderer/global/ui/primitives/button';
import { useUpdateCredentialTokenForm } from '#renderer/domains/settings/runtime/useUpdateCredentialTokenForm';

interface UpdateCredentialTokenFormProps {
  credentialId: number;
  onClose: () => void;
}

export function UpdateCredentialTokenForm({
  credentialId,
  onClose,
}: UpdateCredentialTokenFormProps): React.ReactElement {
  const form = useUpdateCredentialTokenForm(credentialId, onClose);

  return (
    <div className="mt-2 space-y-3 rounded-md border border-border bg-background px-3 py-3">
      <p className="text-xs text-muted-foreground/70">
        Run <code className="rounded bg-muted px-1 py-0.5">claude setup-token</code> in a terminal,
        then paste the new token here.
      </p>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Setup Token</Label>
        <Input
          data-testid="update-token-input"
          type="text"
          value={form.fields.token}
          onChange={(e) => form.fields.setToken(e.target.value)}
          placeholder="Paste setup token..."
          autoFocus
        />
      </div>
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!form.fields.token.trim() || form.state.pending}
          onClick={() => void form.actions.handleUpdate()}
        >
          {form.state.pending ? 'Updating...' : 'Update Token'}
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
