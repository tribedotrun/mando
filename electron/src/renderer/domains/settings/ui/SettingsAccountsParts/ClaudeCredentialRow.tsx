import React, { useState } from 'react';
import {
  CredentialActions,
  CredentialExpiry,
  StatusBadge,
  TokenDisplay,
  UpdateCredentialTokenForm,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import { CredentialUsage } from '#renderer/domains/settings/ui/CredentialUsage';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface ClaudeCredentialRowProps {
  cred: CredentialInfo;
  onRemove: () => void;
  onSetDisabled: (disabled: boolean) => void;
  removePending: boolean;
  setDisabledPending: boolean;
}

export function ClaudeCredentialRow({
  cred,
  onRemove,
  onSetDisabled,
  removePending,
  setDisabledPending,
}: ClaudeCredentialRowProps): React.ReactElement {
  const [showTokenEditor, setShowTokenEditor] = useState(false);

  return (
    <div className="rounded-lg border border-border bg-background px-4 py-3">
      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-medium text-foreground">{cred.label}</span>
            <StatusBadge cred={cred} />
            <CredentialExpiry expiresAt={cred.expiresAt} />
          </div>
          <TokenDisplay cred={cred} />
        </div>
        <CredentialActions
          isDisabled={cred.isDisabled}
          onRemove={onRemove}
          onSetDisabled={onSetDisabled}
          onUpdateAuth={() => setShowTokenEditor((prev) => !prev)}
          removePending={removePending}
          setDisabledPending={setDisabledPending}
        />
      </div>
      <CredentialUsage cred={cred} />
      {showTokenEditor ? (
        <UpdateCredentialTokenForm
          credentialId={cred.id}
          onClose={() => setShowTokenEditor(false)}
        />
      ) : null}
    </div>
  );
}
