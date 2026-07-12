import React from 'react';
import { KeyRound } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';
import {
  AddCredentialForm,
  ClaudeCredentialRow,
  ShowAddButton,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface ClaudeCredentialsSectionProps {
  items: CredentialInfo[];
  isLoading: boolean;
  showInput: boolean;
  setShowInput: (next: boolean) => void;
  onRemove: (id: number) => void;
  onSetDisabled: (id: number, disabled: boolean) => void;
  removePending: boolean;
  setDisabledPending: boolean;
}

export function ClaudeCredentialsSection({
  items,
  isLoading,
  showInput,
  setShowInput,
  onRemove,
  onSetDisabled,
  removePending,
  setDisabledPending,
}: ClaudeCredentialsSectionProps): React.ReactElement {
  return (
    <div data-testid="settings-credentials-claude" className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Claude</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Additional Claude Code credentials for worker load balancing. When no credentials are
          configured, workers use your current Claude Code login.
        </p>
      </div>
      <Card className="py-4">
        <CardContent>
          {isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-full" />
            </div>
          ) : items.length === 0 ? (
            <div className="flex flex-col items-center gap-3 py-8 text-center">
              <KeyRound size={32} className="text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">No Claude credentials configured</p>
            </div>
          ) : (
            <div className="space-y-3">
              {items.map((cred) => (
                <ClaudeCredentialRow
                  key={cred.id}
                  cred={cred}
                  onRemove={() => onRemove(cred.id)}
                  onSetDisabled={(disabled) => onSetDisabled(cred.id, disabled)}
                  removePending={removePending}
                  setDisabledPending={setDisabledPending}
                />
              ))}
            </div>
          )}
        </CardContent>
      </Card>
      <Card className="py-4">
        <CardContent>
          <h3 className="mb-4 text-sm font-medium text-muted-foreground">Add Claude Credential</h3>
          {!showInput ? (
            <ShowAddButton onClick={() => setShowInput(true)} />
          ) : (
            <AddCredentialForm onClose={() => setShowInput(false)} />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
