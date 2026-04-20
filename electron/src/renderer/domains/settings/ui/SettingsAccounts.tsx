import React from 'react';
import { KeyRound, Trash2 } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Button } from '#renderer/global/ui/button';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { useSettingsAccounts } from '#renderer/domains/settings/runtime/useSettingsAccounts';
import {
  AddCredentialForm,
  CredentialExpiry,
  ShowAddButton,
  StatusBadge,
  TokenDisplay,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import { CredentialUsage } from '#renderer/domains/settings/ui/SettingsAccountsUsage';

export function SettingsAccounts(): React.ReactElement {
  const {
    setupToken,
    setSetupToken,
    setupLabel,
    setSetupLabel,
    showTokenInput,
    setShowTokenInput,
    credentials,
    isLoading,
    addTokenMut,
    removeMut,
    handleCancel,
    handleAdd,
  } = useSettingsAccounts();

  return (
    <div data-testid="settings-credentials" className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Credentials</h2>
        <p className="mt-1 text-sm text-muted-foreground">
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
          ) : credentials.length === 0 ? (
            <div className="flex flex-col items-center gap-3 py-8 text-center">
              <KeyRound size={32} className="text-muted-foreground/40" />
              <div>
                <p className="text-sm text-muted-foreground">No credentials configured</p>
                <p className="mt-1 text-xs text-muted-foreground/70">
                  Workers will use your current Claude Code login
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              {credentials.map((cred) => (
                <div
                  key={cred.id}
                  className="rounded-lg border border-border bg-background px-4 py-3"
                >
                  <div className="flex items-start justify-between">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium text-foreground">
                          {cred.label}
                        </span>
                        <StatusBadge cred={cred} />
                        <CredentialExpiry expiresAt={cred.expiresAt} />
                      </div>
                      <TokenDisplay cred={cred} />
                    </div>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="ml-2 shrink-0 text-muted-foreground hover:text-destructive"
                      disabled={removeMut.isPending}
                      onClick={() => removeMut.mutate(cred.id)}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                  <CredentialUsage cred={cred} />
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="py-4">
        <CardContent>
          <h3 className="mb-4 text-sm font-medium text-muted-foreground">Add Credential</h3>
          {!showTokenInput ? (
            <ShowAddButton onClick={() => setShowTokenInput(true)} />
          ) : (
            <AddCredentialForm
              setupToken={setupToken}
              setupLabel={setupLabel}
              isPending={addTokenMut.isPending}
              onTokenChange={setSetupToken}
              onLabelChange={setSetupLabel}
              onAdd={handleAdd}
              onCancel={handleCancel}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
