import React from 'react';
import { CodexBrowserLoginButton } from '#renderer/domains/settings/ui/SettingsAccountsParts';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

export function CredentialExpiredNotice({ cred }: { cred: CredentialInfo }): React.ReactElement {
  if (cred.provider === 'codex') {
    return (
      <div className="mt-2 space-y-2 rounded-md border border-dashed border-destructive/40 px-3 py-2 text-xs text-destructive">
        <CodexBrowserLoginButton credentialId={cred.id} compact />
        <p>or update the auth manually with the key icon above.</p>
      </div>
    );
  }
  return (
    <div className="mt-2 rounded-md border border-dashed border-destructive/40 px-3 py-2 text-xs text-destructive">
      Re-login required: run <code>claude setup-token</code> in a terminal, then use the key icon
      above to paste the new token.
    </div>
  );
}
