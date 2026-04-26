import React from 'react';
import { useSettingsAccounts } from '#renderer/domains/settings/runtime/useSettingsAccounts';
import {
  ClaudeCredentialsSection,
  CodexCredentialsSection,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';

export function SettingsAccounts(): React.ReactElement {
  const accounts = useSettingsAccounts();

  return (
    <div data-testid="settings-credentials" className="space-y-10">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Credentials</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Per-account Claude and Codex credentials. Probes plan/usage every 10 minutes.
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
      <CodexCredentialsSection
        items={accounts.codex.items}
        isLoading={accounts.codex.isLoading}
        matchedCredentialId={accounts.codex.matchedCredentialId}
        showInput={accounts.visibility.showCodexInput}
        setShowInput={accounts.visibility.setShowCodexInput}
        onRemove={(id) => accounts.mutations.removeMut.mutate(id)}
        removePending={accounts.mutations.removeMut.isPending}
        onActivate={(id) => accounts.mutations.codexActivateMut.mutateAsync(id)}
        activatePending={accounts.mutations.codexActivateMut.isPending}
      />
    </div>
  );
}
