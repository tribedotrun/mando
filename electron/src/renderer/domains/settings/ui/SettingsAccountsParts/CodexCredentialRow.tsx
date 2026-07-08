import React from 'react';
import { Badge } from '#renderer/global/ui/primitives/badge';
import {
  CredentialActions,
  CredentialExpiry,
  StatusBadge,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import { CredentialUsage } from '#renderer/domains/settings/ui/CredentialUsage';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface CodexCredentialRowProps {
  cred: CredentialInfo;
  onRemove: () => void;
  onSetDisabled: (disabled: boolean) => void;
  removePending: boolean;
  setDisabledPending: boolean;
}

export function CodexCredentialRow({
  cred,
  onRemove,
  onSetDisabled,
  removePending,
  setDisabledPending,
}: CodexCredentialRowProps): React.ReactElement {
  return (
    <div className="rounded-lg border border-border bg-background px-4 py-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium text-foreground">{cred.label}</span>
            {cred.codex?.planType ? <Badge variant="secondary">{cred.codex.planType}</Badge> : null}
            <StatusBadge cred={cred} />
            <CredentialExpiry expiresAt={cred.expiresAt} />
          </div>
          {cred.codex?.accountId ? (
            <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground/70">
              {cred.codex.accountId}
            </p>
          ) : null}
        </div>
        <CredentialActions
          isDisabled={cred.isDisabled}
          onRemove={onRemove}
          onSetDisabled={onSetDisabled}
          removePending={removePending}
          setDisabledPending={setDisabledPending}
        />
      </div>
      <CredentialUsage cred={cred} />
    </div>
  );
}
