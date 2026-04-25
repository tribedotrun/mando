import React from 'react';

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

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
