import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function CheckboxItem({
  checked,
  depth,
  text,
}: {
  checked: boolean;
  depth: number;
  text: string;
}): React.ReactElement {
  return (
    <div
      className="flex items-start gap-2 py-1 text-body"
      style={{ paddingLeft: `${4 + depth * 16}px` }}
    >
      <span
        className="mt-0.5 inline-block h-3.5 w-3.5 shrink-0 rounded-sm text-center text-label leading-[14px]"
        style={{
          border: '1px solid var(--border)',
          background: checked ? 'var(--foreground)' : 'transparent',
          color: checked ? 'var(--background)' : 'transparent',
        }}
      >
        {checked ? '✓' : ''}
      </span>
      <span className="text-foreground">
        <InlineMarkdown text={text} />
      </span>
    </div>
  );
}
