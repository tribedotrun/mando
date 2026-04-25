import React from 'react';
import type { McpToolName, OpaqueInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';
import { prettyJson } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface McpToolBlockProps {
  id: string;
  name: McpToolName;
  input: OpaqueInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function McpToolBlock({
  id,
  name,
  input,
  result,
  isError,
}: McpToolBlockProps): React.ReactElement {
  const label = `${name.server} · ${name.tool}`;
  return (
    <ToolFrame id={id} name="MCP" summary={label} isError={isError}>
      <pre className="mt-2 max-h-40 overflow-auto rounded bg-muted/60 p-2 font-mono text-label text-foreground">
        {prettyJson(input.raw)}
      </pre>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
