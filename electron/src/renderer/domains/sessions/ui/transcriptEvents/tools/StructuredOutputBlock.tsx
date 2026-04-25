import React from 'react';
import type { StructuredOutputInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { prettyJson } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface StructuredOutputBlockProps {
  id: string;
  input: StructuredOutputInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function StructuredOutputBlock({
  id,
  input,
  isError,
}: StructuredOutputBlockProps): React.ReactElement {
  return (
    <ToolFrame
      id={id}
      name="StructuredOutput"
      summary="structured payload"
      isError={isError}
      defaultOpen
    >
      <pre className="mt-2 max-h-60 overflow-auto rounded bg-muted/60 p-2 font-mono text-label text-foreground">
        {prettyJson(input.raw)}
      </pre>
    </ToolFrame>
  );
}
