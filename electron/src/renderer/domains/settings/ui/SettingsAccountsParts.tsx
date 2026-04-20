import React, { useState } from 'react';
import { Eye, EyeOff, Plus } from 'lucide-react';
import { Input } from '#renderer/global/ui/input';
import { Label } from '#renderer/global/ui/label';
import { Button } from '#renderer/global/ui/button';
import { CopyBtn } from '#renderer/global/ui/CopyBtn';
import { useCredentialReveal, type CredentialInfo } from '#renderer/domains/settings/runtime/hooks';
import { formatExpiry } from '#renderer/domains/settings/service/formatters';

export function StatusBadge({ cred }: { cred: CredentialInfo }): React.ReactElement {
  if (cred.isExpired) {
    return (
      <span className="inline-flex items-center rounded-full bg-destructive/10 px-2 py-0.5 text-xs text-destructive">
        Expired
      </span>
    );
  }
  if (cred.isRateLimited) {
    return (
      <span className="inline-flex items-center rounded-full bg-warning/10 px-2 py-0.5 text-xs text-warning">
        Rate limited
      </span>
    );
  }
  return (
    <span className="inline-flex items-center rounded-full bg-success/10 px-2 py-0.5 text-xs text-success">
      Active
    </span>
  );
}

export function TokenDisplay({ cred }: { cred: CredentialInfo }): React.ReactElement {
  const [revealed, setRevealed] = useState(false);
  const revealMut = useCredentialReveal();

  const toggleReveal = () => {
    if (revealed) {
      setRevealed(false);
      return;
    }
    if (revealMut.data) {
      setRevealed(true);
      return;
    }
    revealMut.mutate(cred.id, { onSuccess: () => setRevealed(true) });
  };

  const fullToken = revealMut.data?.token ?? null;
  const displayToken = revealed && fullToken ? fullToken : cred.tokenMasked;
  const copyToken = revealed && fullToken ? fullToken : cred.tokenMasked;

  return (
    <div className="mt-1 flex items-center gap-1">
      <code className="text-xs text-muted-foreground">{displayToken}</code>
      <Button
        variant="ghost"
        size="icon-xs"
        className="shrink-0 text-muted-foreground"
        disabled={revealMut.isPending}
        onClick={() => void toggleReveal()}
      >
        {revealed ? <EyeOff size={12} /> : <Eye size={12} />}
      </Button>
      <CopyBtn text={copyToken} label="Copy token" />
    </div>
  );
}

export function CredentialExpiry({
  expiresAt,
}: {
  expiresAt: number | null;
}): React.ReactElement | null {
  const expiry = formatExpiry(expiresAt);
  if (!expiry) return null;
  return <span className="text-xs text-muted-foreground">{expiry}</span>;
}

interface AddCredentialFormProps {
  setupToken: string;
  setupLabel: string;
  isPending: boolean;
  onTokenChange: (v: string) => void;
  onLabelChange: (v: string) => void;
  onAdd: () => void;
  onCancel: () => void;
}

export function AddCredentialForm({
  setupToken,
  setupLabel,
  isPending,
  onTokenChange,
  onLabelChange,
  onAdd,
  onCancel,
}: AddCredentialFormProps): React.ReactElement {
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground/70">
        Run <code className="rounded bg-muted px-1 py-0.5">claude setup-token</code> in a terminal
        to generate a token from another account.
      </p>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Label</Label>
        <Input
          data-testid="setup-label-input"
          type="text"
          value={setupLabel}
          onChange={(e) => onLabelChange(e.target.value)}
          placeholder="e.g. team-account-2"
          autoFocus
        />
      </div>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Setup Token</Label>
        <Input
          data-testid="setup-token-input"
          type="text"
          value={setupToken}
          onChange={(e) => onTokenChange(e.target.value)}
          placeholder="Paste setup token..."
        />
      </div>
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!setupToken.trim() || !setupLabel.trim() || isPending}
          onClick={onAdd}
        >
          {isPending ? 'Adding...' : 'Add Credential'}
        </Button>
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={isPending}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

export function ShowAddButton({ onClick }: { onClick: () => void }): React.ReactElement {
  return (
    <Button variant="outline" size="sm" onClick={onClick} className="gap-2">
      <Plus size={14} />
      Add Setup Token
    </Button>
  );
}
