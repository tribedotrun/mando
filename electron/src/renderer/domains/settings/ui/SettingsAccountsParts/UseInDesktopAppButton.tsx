import React from 'react';
import { Loader2, Monitor } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { useUseInDesktopApp } from '#renderer/domains/settings/runtime/useFeedbackCodexDesktopApp';

interface UseInDesktopAppButtonProps {
  credentialLabel: string;
}

export function UseInDesktopAppButton({
  credentialLabel,
}: UseInDesktopAppButtonProps): React.ReactElement {
  const { useInDesktopApp, pending, anySwapInFlight } = useUseInDesktopApp();

  return (
    <Button
      variant="ghost"
      size="icon"
      className="text-muted-foreground hover:text-foreground"
      disabled={pending || anySwapInFlight}
      onClick={() => {
        void useInDesktopApp(credentialLabel);
      }}
      title="Use in desktop app"
      aria-label="Use in desktop app"
    >
      {pending ? <Loader2 size={14} className="animate-spin" /> : <Monitor size={14} />}
    </Button>
  );
}
