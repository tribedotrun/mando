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
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 className="text-lg font-semibold text-foreground">Credentials</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Per-account Claude and Codex credentials. Probes plan/usage every 10 minutes.
          </p>
        </div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          Sort by
          <select
            aria-label="Sort credentials"
            data-testid="credentials-sort"
            value={accounts.sort}
            onChange={(event) => {
              const value = event.target.value;
              if (value === 'reset' || value === 'alphabetical') accounts.setSort(value);
            }}
            className="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <option value="reset">7d reset (soonest)</option>
            <option value="alphabetical">Alphabetical</option>
          </select>
        </label>
      </div>
      <ClaudeCredentialsSection
        items={accounts.claude.items}
        isLoading={accounts.claude.isLoading}
        showInput={accounts.visibility.showTokenInput}
        setShowInput={accounts.visibility.setShowTokenInput}
        onRemove={(id) => accounts.mutations.removeMut.mutate(id)}
        onSetDisabled={(id, disabled) => accounts.mutations.setDisabledMut.mutate({ id, disabled })}
        removePending={accounts.mutations.removeMut.isPending}
        setDisabledPending={accounts.mutations.setDisabledMut.isPending}
      />
      <CodexCredentialsSection
        items={accounts.codex.items}
        isLoading={accounts.codex.isLoading}
        showInput={accounts.visibility.showCodexInput}
        setShowInput={accounts.visibility.setShowCodexInput}
        onRemove={(id) => accounts.mutations.removeMut.mutate(id)}
        onSetDisabled={(id, disabled) => accounts.mutations.setDisabledMut.mutate({ id, disabled })}
        removePending={accounts.mutations.removeMut.isPending}
        setDisabledPending={accounts.mutations.setDisabledMut.isPending}
      />
    </div>
  );
}
