import React from 'react';
import type { SkillInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { ToolResultBody } from '#renderer/domains/sessions/ui/transcriptEvents/ToolResultBody';

interface SkillBlockProps {
  id: string;
  input: SkillInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function SkillBlock({ id, input, result, isError }: SkillBlockProps): React.ReactElement {
  const summary = input.args ? `/${input.skill} ${input.args}` : `/${input.skill}`;
  return (
    <ToolFrame id={id} name="Skill" summary={summary} isError={isError}>
      <ToolResultBody result={result} />
    </ToolFrame>
  );
}
