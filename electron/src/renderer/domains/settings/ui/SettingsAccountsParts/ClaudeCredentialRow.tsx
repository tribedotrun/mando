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
import { Switch } from '#renderer/global/ui/primitives/switch';
import { useCredentialCliEligibility } from '#renderer/domains/settings/runtime/useFeedbackCredentials';
import { ClaudeDesktopProfileControls } from '#renderer/domains/settings/ui/SettingsAccountsParts/ClaudeDesktopProfileControls';

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
  const cliEligibility = useCredentialCliEligibility();

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
      <label className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
        <Switch
          size="sm"
          aria-label={`Use ${cred.label} for CLI`}
          checked={cred.cliEligible}
          disabled={cliEligibility.isPending}
          onCheckedChange={(cliEligible) => cliEligibility.mutate({ id: cred.id, cliEligible })}
        />
        Use for CLI and worker load balancing
      </label>
      <ClaudeDesktopProfileControls credentialId={cred.id} />
      {showTokenEditor ? (
        <UpdateCredentialTokenForm
          credentialId={cred.id}
          onClose={() => setShowTokenEditor(false)}
        />
      ) : null}
    </div>
  );
}
