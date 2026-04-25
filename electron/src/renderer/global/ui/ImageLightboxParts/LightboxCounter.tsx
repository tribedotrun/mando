import React from 'react';

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

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
