import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function BulletItem({ depth, text }: { depth: number; text: string }): React.ReactElement {
  return (
    <div className="flex gap-2 py-1 text-body" style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <span className="text-text-3">&bull;</span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}
