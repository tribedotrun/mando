import React from 'react';
import { X } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

export function LightboxCloseButton({ onClose }: { onClose: () => void }): React.ReactElement {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={onClose}
      className="fixed right-4 top-4 z-[201] rounded-full text-foreground"
      style={{ background: CONTROL_BG }}
      aria-label="Close"
    >
      <X size={14} strokeWidth={2} />
    </Button>
  );
}
