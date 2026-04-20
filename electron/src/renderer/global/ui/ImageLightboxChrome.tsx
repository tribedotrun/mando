import React from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { formatZoomPercent } from '#renderer/global/service/lightboxHelpers';
import { Button } from '#renderer/global/ui/button';

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

export function LightboxCounter({
  index,
  total,
}: {
  index: number;
  total: number;
}): React.ReactElement {
  return (
    <div
      className="fixed left-1/2 top-4 z-[201] -translate-x-1/2 rounded-full px-3 py-1 text-[12px] text-muted-foreground"
      style={{ background: CONTROL_BG }}
    >
      {index + 1} / {total}
    </div>
  );
}

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

export function LightboxZoomIndicator({ zoom }: { zoom: number }): React.ReactElement {
  return (
    <div
      className="fixed bottom-4 left-1/2 z-[201] -translate-x-1/2 rounded-full px-3 py-1 text-[12px] text-muted-foreground"
      style={{ background: CONTROL_BG }}
    >
      {formatZoomPercent(zoom)}
    </div>
  );
}

export function LightboxCaption({ text }: { text: string }): React.ReactElement {
  return (
    <div
      className="fixed bottom-4 left-1/2 z-[201] -translate-x-1/2 rounded-full px-4 py-1.5 text-[13px] text-foreground"
      style={{ background: CONTROL_BG }}
    >
      {text}
    </div>
  );
}
