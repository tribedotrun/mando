import React from 'react';
import type { OtherToolName, OpaqueInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';
import { prettyJson } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface OtherToolBlockProps {
  id: string;
  name: OtherToolName;
  input: OpaqueInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function OtherToolBlock({
  id,
  name,
  input,
  result,
  isError,
}: OtherToolBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name={name.name} summary="unknown tool" isError={isError}>
      <pre className="mt-2 max-h-40 overflow-auto rounded bg-muted/60 p-2 font-mono text-label text-foreground">
        {prettyJson(input.raw)}
      </pre>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
