import React, { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { CopyBtn } from '#renderer/domains/settings/ui/CopyBtn';
import { useCredentialReveal, type CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

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
