import React from 'react';
import type { GrepInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';
import { buildGrepSummary } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface GrepBlockProps {
  id: string;
  input: GrepInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function GrepBlock({ id, input, result, isError }: GrepBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="Grep" summary={buildGrepSummary(input)} isError={isError}>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
