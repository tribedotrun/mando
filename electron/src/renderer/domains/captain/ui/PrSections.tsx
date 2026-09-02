import React from 'react';
import { stripBotContent } from '#renderer/domains/captain/service/prHelpers';
import { GitHubPrMarkdown } from '#renderer/domains/captain/ui/GitHubPrMarkdown';

interface Props {
  text: string;
}

export function PrSections({ text }: Props): React.ReactElement | null {
  const cleaned = stripBotContent(text);
  if (!cleaned) {
    return <span className="text-[12px] italic text-text-3">No description</span>;
  }
  return <GitHubPrMarkdown text={cleaned} />;
}
