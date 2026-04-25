import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function AdmonitionBlock({
  type,
  bodyLines,
}: {
  type: string;
  bodyLines: string[];
}): React.ReactElement {
  const admonitionColors: Record<string, string> = {
    NOTE: 'var(--muted-foreground)',
    TIP: 'var(--muted-foreground)',
    IMPORTANT: 'var(--muted-foreground)',
    WARNING: 'var(--stale)',
    CAUTION: 'var(--destructive)',
  };
  const color = admonitionColors[type] ?? 'var(--border)';

  return (
    <div
      className="my-2 rounded px-3 py-2 text-body [overflow-wrap:anywhere]"
      style={{
        background: `color-mix(in srgb, ${color} 8%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 25%, transparent)`,
      }}
    >
      <div className="mb-1 text-label" style={{ color }}>
        {type}
      </div>
      {bodyLines.map((bl, bi) => (
        <div key={bi} className="text-muted-foreground">
          <InlineMarkdown text={bl} />
        </div>
      ))}
    </div>
  );
}
