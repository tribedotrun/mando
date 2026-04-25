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
  const summary = input.subagentType
    ? `${input.description} · ${input.subagentType}`
    : input.description;
  return (
    <ToolFrame id={id} name="Task" summary={summary} isError={isError}>
      <p className="mt-2 whitespace-pre-wrap text-label text-muted-foreground">{input.prompt}</p>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
