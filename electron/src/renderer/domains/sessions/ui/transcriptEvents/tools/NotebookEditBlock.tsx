import React from 'react';
import type { NotebookEditInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface NotebookEditBlockProps {
  id: string;
  input: NotebookEditInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function NotebookEditBlock({
  id,
  input,
  result,
  isError,
}: NotebookEditBlockProps): React.ReactElement {
  const mode = input.editMode ?? 'replace';
  const summary = `${input.notebookPath} · ${mode}`;
  return (
    <ToolFrame id={id} name="NotebookEdit" summary={summary} isError={isError}>
      <pre className="mt-2 max-h-60 overflow-auto rounded bg-muted/60 p-2 font-mono text-label text-foreground">
        {input.newSource}
      </pre>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
