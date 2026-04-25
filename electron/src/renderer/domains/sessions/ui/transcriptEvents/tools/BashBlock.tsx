import React from 'react';
import type { BashInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface BashBlockProps {
  id: string;
  input: BashInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function BashBlock({ id, input, result, isError }: BashBlockProps): React.ReactElement {
  const summary = input.description ?? input.command;
  return (
    <ToolFrame id={id} name="Bash" summary={summary} isError={isError}>
      <pre className="mt-2 overflow-auto rounded bg-black/40 p-2 font-mono text-label text-foreground">
        {input.command}
      </pre>
      <ToolResultBody result={result} fallbackLang="bash" />
    </ToolFrame>
  );
}
