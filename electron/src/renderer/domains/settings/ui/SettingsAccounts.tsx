import React from 'react';
import { useSettingsAccounts } from '#renderer/domains/settings/runtime/useSettingsAccounts';
import { ClaudeCredentialsSection } from '#renderer/domains/settings/ui/SettingsAccountsParts';

export function SettingsAccounts(): React.ReactElement {
  const accounts = useSettingsAccounts();

  return (
    <div data-testid="settings-credentials" className="space-y-10">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Credentials</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Per-account Claude credentials. Probes plan/usage every 10 minutes.
        </p>
      </div>
      <ClaudeCredentialsSection
        items={accounts.claude.items}
        isLoading={accounts.claude.isLoading}
        showInput={accounts.visibility.showTokenInput}
        setShowInput={accounts.visibility.setShowTokenInput}
        onRemove={(id) => accounts.mutations.removeMut.mutate(id)}
        removePending={accounts.mutations.removeMut.isPending}
      />
    </div>
  );
}
