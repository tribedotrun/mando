import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function NumberedItem({
  num,
  depth,
  text,
}: {
  num: string;
  depth: number;
  text: string;
}): React.ReactElement {
  return (
    <div className="flex gap-2 py-1 text-body" style={{ paddingLeft: `${8 + depth * 16}px` }}>
      <span className="w-4 shrink-0 text-right text-text-3">{num}.</span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}
