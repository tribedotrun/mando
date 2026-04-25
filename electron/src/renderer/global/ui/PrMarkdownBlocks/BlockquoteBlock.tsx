import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function BlockquoteBlock({ lines }: { lines: string[] }): React.ReactElement {
  return (
    <div className="my-1 border-l-2 border-muted-foreground/30 pl-3 text-body italic text-text-3">
      {lines.map((bl, bi) => (
        <div key={bi}>
          <InlineMarkdown text={bl} />
        </div>
      ))}
    </div>
  );
}
