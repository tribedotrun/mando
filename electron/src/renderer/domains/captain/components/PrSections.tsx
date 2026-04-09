import React from 'react';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';

/** Strip the Devin review badge and trailing HRs before it. */
function stripBadge(text: string): string {
  return text
    .replace(/<!-- devin-review-badge-begin -->[\s\S]*?<!-- devin-review-badge-end -->/g, '')
    .replace(/<!-- pr-summary-head:.*?-->/g, '')
    .replace(/(\n\s*---\s*)+\s*$/g, '')
    .trim();
}

interface Props {
  text: string;
}

export function PrSections({ text }: Props): React.ReactElement | null {
  const cleaned = stripBadge(text);
  if (!cleaned) {
    return <span className="text-[12px] italic text-text-3">No description</span>;
  }
  return <PrMarkdown text={cleaned} />;
}
