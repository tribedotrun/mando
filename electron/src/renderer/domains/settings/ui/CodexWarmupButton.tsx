import React from 'react';
import { TimerReset } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { useCodexCredentialWarmup } from '#renderer/domains/settings/runtime/hooks';

interface CodexWarmupButtonProps {
  credentialId: number;
  disabled: boolean;
}

/**
 * Manual trigger for a Codex usage warm-up: one throwaway prompt that starts
 * the credential's rolling rate-limit windows. The daemon fires the same
 * warm-up automatically whenever a credential is probed at zero usage.
 */
export function CodexWarmupButton({
  credentialId,
  disabled,
}: CodexWarmupButtonProps): React.ReactElement {
  const warmupMut = useCodexCredentialWarmup();
  return (
    <Button
      variant="ghost"
      size="icon-xs"
      onClick={() => warmupMut.mutate(credentialId)}
      disabled={disabled || warmupMut.isPending}
      title="Start usage clock (sends one throwaway prompt)"
      aria-label="Start Codex usage clock"
      data-testid="codex-warmup-button"
    >
      <TimerReset size={12} className={warmupMut.isPending ? 'animate-pulse' : undefined} />
    </Button>
  );
}
