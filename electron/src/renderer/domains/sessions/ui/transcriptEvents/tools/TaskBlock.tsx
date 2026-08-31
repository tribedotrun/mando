import React from 'react';
import type { TaskInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface TaskBlockProps {
  id: string;
  input: TaskInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function TaskBlock({ id, input, result, isError }: TaskBlockProps): React.ReactElement {
  const isExploration = input.prompt.trim() === '';
  const summary = input.description || input.subagentType || 'subagent';
  return (
    <ToolFrame
      id={id}
      name={isExploration ? 'Explored' : 'Task'}
      summary={summary}
      isError={isError}
    >
      {input.prompt && (
        <p className="mt-2 whitespace-pre-wrap text-label text-muted-foreground">{input.prompt}</p>
      )}
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
