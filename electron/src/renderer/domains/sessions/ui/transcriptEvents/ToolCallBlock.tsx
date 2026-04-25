import React from 'react';
import type { AssistantToolUseBlock, UserToolResultBlock } from '#renderer/global/types';
import { BashBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/BashBlock';
import { EditDiffBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/EditDiffBlock';
import { GlobBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/GlobBlock';
import { GrepBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/GrepBlock';
import { McpToolBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/McpToolBlock';
import { NotebookEditBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/NotebookEditBlock';
import { OtherToolBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/OtherToolBlock';
import { ReadBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/ReadBlock';
import { SkillBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/SkillBlock';
import { StructuredOutputBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/StructuredOutputBlock';
import { TaskBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/TaskBlock';
import { TodoWriteBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/TodoWriteBlock';
import { WebFetchBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/WebFetchBlock';
import { WebSearchBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/WebSearchBlock';
import { WriteBlock } from '#renderer/domains/sessions/ui/transcriptEvents/tools/WriteBlock';

interface ToolCallBlockProps {
  toolUse: AssistantToolUseBlock;
  result?: UserToolResultBlock;
}

export function ToolCallBlock({ toolUse, result }: ToolCallBlockProps): React.ReactElement | null {
  const { id, name, input } = toolUse;
  const isError = result?.isError === true;

  if (name.kind === 'bash' && input.kind === 'bash') {
    return <BashBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'read' && input.kind === 'read') {
    return <ReadBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'edit' && input.kind === 'edit') {
    return <EditDiffBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'write' && input.kind === 'write') {
    return <WriteBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'grep' && input.kind === 'grep') {
    return <GrepBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'glob' && input.kind === 'glob') {
    return <GlobBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'todo_write' && input.kind === 'todo_write') {
    return <TodoWriteBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'web_fetch' && input.kind === 'web_fetch') {
    return <WebFetchBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'web_search' && input.kind === 'web_search') {
    return <WebSearchBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'task' && input.kind === 'task') {
    return <TaskBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'notebook_edit' && input.kind === 'notebook_edit') {
    return <NotebookEditBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'skill' && input.kind === 'skill') {
    return <SkillBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'structured_output' && input.kind === 'structured_output') {
    return <StructuredOutputBlock id={id} input={input.data} result={result} isError={isError} />;
  }
  if (name.kind === 'mcp' && input.kind === 'opaque') {
    return (
      <McpToolBlock id={id} name={name.data} input={input.data} result={result} isError={isError} />
    );
  }
  if (name.kind === 'other' && input.kind === 'opaque') {
    return (
      <OtherToolBlock
        id={id}
        name={name.data}
        input={input.data}
        result={result}
        isError={isError}
      />
    );
  }
  return null;
}
