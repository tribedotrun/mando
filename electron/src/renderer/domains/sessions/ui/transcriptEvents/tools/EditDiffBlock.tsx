import React from 'react';
import type { EditInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface EditDiffBlockProps {
  id: string;
  input: EditInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function EditDiffBlock({
  id,
  input,
  result,
  isError,
}: EditDiffBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="Edit" summary={input.filePath} isError={isError} defaultOpen>
      <div className="mt-2 overflow-auto rounded bg-muted/60 p-2 font-mono text-label">
        {input.oldString.split('\n').map((line, i) => (
          <div key={`o-${i}`} className="text-destructive">
            - {line}
          </div>
        ))}
        {input.newString.split('\n').map((line, i) => (
          <div key={`n-${i}`} className="text-emerald-400">
            + {line}
          </div>
        ))}
      </div>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
