import React from 'react';
import { KeyRound } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';
import {
  AddCodexCredentialForm,
  CodexCredentialRow,
  ShowAddButton,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface CodexCredentialsSectionProps {
  items: CredentialInfo[];
  isLoading: boolean;
  showInput: boolean;
  setShowInput: (next: boolean) => void;
  onRemove: (id: number) => void;
  onSetDisabled: (id: number, disabled: boolean) => void;
  removePending: boolean;
  setDisabledPending: boolean;
}

export function CodexCredentialsSection(props: CodexCredentialsSectionProps): React.ReactElement {
  return (
    <div data-testid="settings-credentials-codex" className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Codex accounts</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          OAuth-only ChatGPT accounts for the Codex CLI. Use
          <code className="mx-1">cx</code> or
          <code className="mx-1">mdo create --codex</code> to load-balance without changing
          <code>~/.codex/auth.json</code> (threads stay in the shared
          <code className="mx-1">~/.codex</code> home, though an account added here that is also
          your personal login shares that account&apos;s rate limits).
        </p>
      </div>
      <Card className="py-4">
        <CardContent>
          {props.isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-12 w-full" />
            </div>
          ) : props.items.length === 0 ? (
            <div className="flex flex-col items-center gap-3 py-8 text-center">
              <KeyRound size={32} className="text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">No Codex accounts configured</p>
            </div>
          ) : (
            <div className="space-y-3">
              {props.items.map((cred) => (
                <CodexCredentialRow
                  key={cred.id}
                  cred={cred}
                  onRemove={() => props.onRemove(cred.id)}
                  onSetDisabled={(disabled) => props.onSetDisabled(cred.id, disabled)}
                  removePending={props.removePending}
                  setDisabledPending={props.setDisabledPending}
                />
              ))}
            </div>
          )}
        </CardContent>
      </Card>
      <Card className="py-4">
        <CardContent>
          <h3 className="mb-4 text-sm font-medium text-muted-foreground">Add Codex Account</h3>
          {!props.showInput ? (
            <ShowAddButton onClick={() => props.setShowInput(true)} />
          ) : (
            <AddCodexCredentialForm onClose={() => props.setShowInput(false)} />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
