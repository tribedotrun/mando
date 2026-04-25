import React from 'react';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function DetailsBlock({
  summaryText,
  children,
}: {
  summaryText: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <details className="my-2 rounded border border-border px-3 py-2">
      <summary className="cursor-pointer text-[12px] font-medium text-foreground select-none">
        <InlineMarkdown text={summaryText} />
      </summary>
      <div className="mt-2 text-[12px]">{children}</div>
    </details>
  );
}
