import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

const HEADING_STYLES: Record<number, string> = Object.freeze({
  1: 'mt-4 mb-2 text-subheading text-foreground',
  2: 'mt-4 mb-1.5 text-body font-semibold text-foreground',
  3: 'mt-3 mb-1 text-body font-semibold text-foreground',
  4: 'mt-3 mb-1 text-caption font-semibold text-foreground',
});

export function HeadingBlock({ level, text }: { level: number; text: string }): React.ReactElement {
  return (
    <div className={HEADING_STYLES[level] ?? HEADING_STYLES[4]}>
      <InlineMarkdown text={text} />
    </div>
  );
}
