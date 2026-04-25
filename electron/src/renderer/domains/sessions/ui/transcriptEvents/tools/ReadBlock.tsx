import React from 'react';
import type { ReadInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';
import { buildReadSummary } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface ReadBlockProps {
  id: string;
  input: ReadInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function ReadBlock({ id, input, result, isError }: ReadBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="Read" summary={buildReadSummary(input)} isError={isError}>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
