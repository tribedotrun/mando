import React, { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGet, apiPost, apiDel } from '#renderer/domains/settings/hooks/useApi';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Input } from '#renderer/components/ui/input';
import { Label } from '#renderer/components/ui/label';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { toast } from 'sonner';
import { Eye, EyeOff, KeyRound, Plus, Trash2 } from 'lucide-react';
import { CopyBtn } from '#renderer/global/components/CopyBtn';
import { queryKeys } from '#renderer/queryKeys';

interface CredentialInfo {
  id: number;
  label: string;
  tokenMasked: string;
  expiresAt: number | null;
  rateLimitCooldownUntil: number | null;
  createdAt: string;
  isExpired: boolean;
  isRateLimited: boolean;
}

interface CredentialListResponse {
  credentials: CredentialInfo[];
}

const QUERY_KEY = queryKeys.credentials.all;

function StatusBadge({ cred }: { cred: CredentialInfo }): React.ReactElement {
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

function formatExpiry(expiresAt: number | null): string | null {
  if (expiresAt == null) return null;
  const now = Date.now();
  const diff = expiresAt - now;
  if (diff <= 0) return 'Expired';
  const hours = (diff / (1000 * 60 * 60)) | 0;
  if (hours < 24) return `${hours}h remaining`;
  const days = (hours / 24) | 0;
  return `${days}d remaining`;
}

function TokenDisplay({ cred }: { cred: CredentialInfo }): React.ReactElement {
  const [revealed, setRevealed] = useState(false);
  const [fullToken, setFullToken] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const toggleReveal = async () => {
    if (revealed) {
      setRevealed(false);
      return;
    }
    if (fullToken) {
      setRevealed(true);
      return;
    }
    setLoading(true);
    try {
      const res = await apiGet<{ token: string }>(`/api/credentials/${cred.id}/token`);
      setFullToken(res.token);
      setRevealed(true);
    } finally {
      setLoading(false);
    }
  };

  const displayToken = revealed && fullToken ? fullToken : cred.tokenMasked;
  const copyToken = fullToken ?? cred.tokenMasked;

  return (
    <div className="mt-1 flex items-center gap-1">
      <code className="text-xs text-muted-foreground">{displayToken}</code>
      <Button
        variant="ghost"
        size="icon-xs"
        className="shrink-0 text-muted-foreground"
        disabled={loading}
        onClick={() => void toggleReveal()}
      >
        {revealed ? <EyeOff size={12} /> : <Eye size={12} />}
      </Button>
      <CopyBtn text={copyToken} label="Copy token" />
    </div>
  );
}

export function SettingsAccounts(): React.ReactElement {
  const qc = useQueryClient();
  const [setupToken, setSetupToken] = useState('');
  const [setupLabel, setSetupLabel] = useState('');
  const [showTokenInput, setShowTokenInput] = useState(false);

  const { data, isLoading } = useQuery<CredentialListResponse>({
    queryKey: QUERY_KEY,
    queryFn: () => apiGet<CredentialListResponse>('/api/credentials'),
    refetchInterval: 30_000,
  });

  const addTokenMut = useMutation({
    mutationFn: ({ label, token }: { label: string; token: string }) =>
      apiPost<{ ok: boolean; id?: number; label?: string }>('/api/credentials/setup-token', {
        label,
        token,
      }),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
      setSetupToken('');
      setSetupLabel('');
      setShowTokenInput(false);
      toast.success(`Credential added: ${res.label}`);
    },
    onError: () => toast.error('Failed to add credential'),
  });

  const removeMut = useMutation({
    mutationFn: (id: number) => apiDel(`/api/credentials/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
      toast.success('Credential removed');
    },
    onError: () => toast.error('Failed to remove credential'),
  });

  const credentials = data?.credentials ?? [];

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
                  className="flex items-center justify-between rounded-lg border border-border bg-background px-4 py-3"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm font-medium text-foreground">
                        {cred.label}
                      </span>
                      <StatusBadge cred={cred} />
                      {(() => {
                        const expiry = formatExpiry(cred.expiresAt);
                        return expiry ? (
                          <span className="text-xs text-muted-foreground">{expiry}</span>
                        ) : null;
                      })()}
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
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="py-4">
        <CardContent>
          <h3 className="mb-4 text-sm font-medium text-muted-foreground">Add Credential</h3>
          {!showTokenInput ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowTokenInput(true)}
              className="gap-2"
            >
              <Plus size={14} />
              Add Setup Token
            </Button>
          ) : (
            <div className="space-y-3">
              <p className="text-xs text-muted-foreground/70">
                Run <code className="rounded bg-muted px-1 py-0.5">claude setup-token</code> in a
                terminal to generate a token from another account.
              </p>
              <div>
                <Label className="mb-1.5 text-xs text-muted-foreground">Label</Label>
                <Input
                  data-testid="setup-label-input"
                  type="text"
                  value={setupLabel}
                  onChange={(e) => setSetupLabel(e.target.value)}
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
                  onChange={(e) => setSetupToken(e.target.value)}
                  placeholder="Paste setup token..."
                />
              </div>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  disabled={!setupToken.trim() || !setupLabel.trim() || addTokenMut.isPending}
                  onClick={() =>
                    addTokenMut.mutate({
                      label: setupLabel.trim(),
                      token: setupToken.trim(),
                    })
                  }
                >
                  {addTokenMut.isPending ? 'Adding...' : 'Add Credential'}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setShowTokenInput(false);
                    setSetupToken('');
                    setSetupLabel('');
                  }}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
