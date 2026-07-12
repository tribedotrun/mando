import React from 'react';
import {
  CodexBrowserLoginButton,
  UpdateCodexAuthPasteForm,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';

interface UpdateCodexCredentialAuthFormProps {
  credentialId: number;
  onClose: () => void;
}

export function UpdateCodexCredentialAuthForm({
  credentialId,
  onClose,
}: UpdateCodexCredentialAuthFormProps): React.ReactElement {
  return (
    <div className="mt-2 space-y-3 rounded-md border border-border bg-background px-3 py-3">
      <div className="space-y-1.5">
        <CodexBrowserLoginButton credentialId={credentialId} compact />
        <p className="text-xs text-muted-foreground/70">
          Sign in with the same ChatGPT account to refresh this credential.
        </p>
      </div>
      <div className="flex items-center gap-2 text-xs text-muted-foreground/70">
        <div className="h-px flex-1 bg-border" />
        or paste auth.json manually
        <div className="h-px flex-1 bg-border" />
      </div>
      <UpdateCodexAuthPasteForm credentialId={credentialId} onClose={onClose} />
    </div>
  );
}
