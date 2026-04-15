import React from 'react';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';

/** Strip review-bot badges, footers, and trailing HRs. */
function stripBotContent(text: string): string {
  return (
    text
      // Devin review badge
      .replace(/<!-- devin-review-badge-begin -->[\s\S]*?<!-- devin-review-badge-end -->/g, '')
      // PR summary head marker
      .replace(/<!-- pr-summary-head:.*?-->/g, '')
      // Cursor agent badge (trailing <div> with cursor.com links after body end marker)
      .replace(/<!-- CURSOR_AGENT_PR_BODY_END -->[\s\S]*/g, '')
      .replace(/<!-- CURSOR_AGENT_PR_BODY_BEGIN -->\n?/g, '')
      // CodeRabbit auto-generated comments
      .replace(
        /<!-- This is an auto-generated comment: release notes by coderabbit\.ai -->[\s\S]*?<!-- end of auto-generated comment: release notes by coderabbit\.ai -->/g,
        '',
      )
      // Greptile comment blocks
      .replace(/<!-- greptile[\s\S]*?-->/g, '')
      // Trailing HRs
      .replace(/(\n\s*---\s*)+\s*$/g, '')
      .trim()
  );
}

interface Props {
  text: string;
}

export function PrSections({ text }: Props): React.ReactElement | null {
  const cleaned = stripBotContent(text);
  if (!cleaned) {
    return <span className="text-[12px] italic text-text-3">No description</span>;
  }
  return <PrMarkdown text={cleaned} />;
}
