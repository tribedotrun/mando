import React from 'react';
import { formatZoomPercent } from '#renderer/global/service/lightboxHelpers';

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

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
