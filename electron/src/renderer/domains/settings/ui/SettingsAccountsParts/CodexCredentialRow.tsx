import React from 'react';
import { Trash2 } from 'lucide-react';
import { Badge } from '#renderer/global/ui/primitives/badge';
import { Button } from '#renderer/global/ui/primitives/button';
import { CredentialExpiry, StatusBadge } from '#renderer/domains/settings/ui/SettingsAccountsParts';
import { CredentialUsage } from '#renderer/domains/settings/ui/CredentialUsage';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface CodexCredentialRowProps {
  cred: CredentialInfo;
  isActive: boolean;
  onRemove: () => void;
  removePending: boolean;
  onActivate: () => void;
  activatePending: boolean;
}

export function CodexCredentialRow({
  cred,
  isActive,
  onRemove,
  removePending,
  onActivate,
  activatePending,
}: CodexCredentialRowProps): React.ReactElement {
  return (
    <div className="rounded-lg border border-border bg-background px-4 py-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium text-foreground">{cred.label}</span>
            {isActive ? (
              <Badge variant="default" data-testid={`codex-cred-active-${cred.id}`}>
                Active
              </Badge>
            ) : null}
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
        <div className="flex items-center gap-1">
          {!isActive ? (
            <Button
              size="sm"
              variant="outline"
              disabled={activatePending}
              onClick={onActivate}
              data-testid={`codex-cred-activate-${cred.id}`}
            >
              {activatePending ? 'Switching...' : 'Make active'}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="icon"
            className="shrink-0 text-muted-foreground hover:text-destructive"
            disabled={removePending}
            onClick={onRemove}
          >
            <Trash2 size={14} />
          </Button>
        </div>
      </div>
      <CredentialUsage cred={cred} />
    </div>
  );
}
