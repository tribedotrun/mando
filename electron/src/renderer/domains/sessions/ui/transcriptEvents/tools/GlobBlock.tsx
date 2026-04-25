import React from 'react';
import type { GlobInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface GlobBlockProps {
  id: string;
  input: GlobInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function GlobBlock({ id, input, result, isError }: GlobBlockProps): React.ReactElement {
  const summary = input.path ? `${input.pattern} in ${input.path}` : input.pattern;
  return (
    <ToolFrame id={id} name="Glob" summary={summary} isError={isError}>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
