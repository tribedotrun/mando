import React from 'react';
import type { UserToolResultBlock, WebSearchInput } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface WebSearchBlockProps {
  id: string;
  input: WebSearchInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function WebSearchBlock({
  id,
  input,
  result,
  isError,
}: WebSearchBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="WebSearch" summary={input.query} isError={isError}>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
