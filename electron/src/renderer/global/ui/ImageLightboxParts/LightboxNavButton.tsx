import React from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

export function LightboxNavButton({
  direction,
  onClick,
}: {
  direction: 'prev' | 'next';
  onClick: () => void;
}): React.ReactElement {
  const isPrev = direction === 'prev';
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={onClick}
      className={`fixed top-1/2 z-[201] -translate-y-1/2 rounded-full text-foreground ${isPrev ? 'left-4' : 'right-4'}`}
      style={{ background: CONTROL_BG }}
      aria-label={isPrev ? 'Previous image' : 'Next image'}
    >
      {isPrev ? (
        <ChevronLeft size={16} strokeWidth={2} />
      ) : (
        <ChevronRight size={16} strokeWidth={2} />
      )}
    </Button>
  );
}
