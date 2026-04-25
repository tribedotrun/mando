import React from 'react';
import type { UserToolResultBlock, WriteInput } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface WriteBlockProps {
  id: string;
  input: WriteInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function WriteBlock({ id, input, result, isError }: WriteBlockProps): React.ReactElement {
  const lineCount = input.content.split('\n').length;
  const summary = `${input.filePath} · ${lineCount} line${lineCount === 1 ? '' : 's'}`;
  return (
    <ToolFrame id={id} name="Write" summary={summary} isError={isError}>
      <pre className="mt-2 max-h-60 overflow-auto rounded bg-muted/60 p-2 font-mono text-label text-foreground">
        {input.content}
      </pre>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
