import React from 'react';
import { KeyRound } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';
import {
  AddCredentialForm,
  ClaudeCredentialRow,
  ShowAddButton,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import {
  useCredentialRouting,
  type CredentialInfo,
} from '#renderer/domains/settings/runtime/hooks';

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
  const leases = useCredentialRouting();
  return (
    <div data-testid="settings-credentials-claude" className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Claude</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Pool Claude accounts across CLI workers and Desktop. Choose which accounts participate in
          CLI load balancing; each Desktop login shares your local Claude files and memory.
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          CLI switching prefers accounts near their weekly reset with room for another session.
          Accounts with similar reset times are balanced by remaining quota and tracked session
          load.
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          In a supervised Claude session, use <code>!mando switch</code> to choose an account or{' '}
          <code>!mando switch --dry-run</code> to see the routing decision. Type <code>/exit</code>{' '}
          after queuing a switch to resume the conversation with the selected account.
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          Local session import is optional under Manage. Opening Desktop does not import sessions.
          Cloud history stays with its account; Desktop requests are not automatically rotated.
        </p>
        {leases.isError ? (
          <p className="mt-1 text-xs text-destructive" role="status">
            Live session counts are unavailable: {leases.error.message}
          </p>
        ) : null}
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
                  leaseCount={leases.data?.candidates.find(
                    (profile) => profile.credential_id === cred.id,
                  )}
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
