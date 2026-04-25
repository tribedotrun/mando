import React from 'react';
import type { UserToolResultBlock, WebFetchInput } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface WebFetchBlockProps {
  id: string;
  input: WebFetchInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function WebFetchBlock({
  id,
  input,
  result,
  isError,
}: WebFetchBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="WebFetch" summary={input.url} isError={isError}>
      <p className="mt-2 text-label text-muted-foreground">{input.prompt}</p>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
