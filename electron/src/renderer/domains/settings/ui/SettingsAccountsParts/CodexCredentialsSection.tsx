import React, { useState } from 'react';
import { KeyRound } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';
import {
  ActivateCodexConfirmModal,
  AddCodexCredentialForm,
  CodexCredentialRow,
  ShowAddButton,
} from '#renderer/domains/settings/ui/SettingsAccountsParts';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

interface CodexCredentialsSectionProps {
  items: CredentialInfo[];
  isLoading: boolean;
  matchedCredentialId: number | null;
  showInput: boolean;
  setShowInput: (next: boolean) => void;
  onRemove: (id: number) => void;
  removePending: boolean;
  onActivate: (id: number) => Promise<{ ok: boolean; accountId: string }>;
  activatePending: boolean;
}

export function CodexCredentialsSection(props: CodexCredentialsSectionProps): React.ReactElement {
  const [confirmLabel, setConfirmLabel] = useState<string | null>(null);

  const handleActivate = async (cred: CredentialInfo) => {
    const result = await props.onActivate(cred.id);
    if (result.ok) setConfirmLabel(cred.label);
  };

  return (
    <div data-testid="settings-credentials-codex" className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Codex accounts</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Each account stores its OpenAI tokens and ChatGPT plan usage. Switching writes only
          <code className="mx-1">~/.codex/auth.json</code> — the rest of <code>~/.codex</code>
          (conversation history, sessions) stays put.
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
              {props.items.map((cred) => {
                const isActive = props.matchedCredentialId === cred.id;
                return (
                  <CodexCredentialRow
                    key={cred.id}
                    cred={cred}
                    isActive={isActive}
                    onRemove={() => props.onRemove(cred.id)}
                    removePending={props.removePending}
                    onActivate={() => void handleActivate(cred)}
                    activatePending={props.activatePending}
                  />
                );
              })}
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
      <ActivateCodexConfirmModal
        open={confirmLabel !== null}
        label={confirmLabel ?? ''}
        onClose={() => setConfirmLabel(null)}
      />
    </div>
  );
}
