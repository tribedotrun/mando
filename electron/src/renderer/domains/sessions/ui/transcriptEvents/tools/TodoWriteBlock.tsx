import React from 'react';
import type { TodoWriteInput, UserToolResultBlock } from '#renderer/global/types';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';
import { todoMarker } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface TodoWriteBlockProps {
  id: string;
  input: TodoWriteInput;
  result?: UserToolResultBlock;
  isError?: boolean;
}

export function TodoWriteBlock({ id, input, isError }: TodoWriteBlockProps): React.ReactElement {
  const total = input.todos.length;
  const done = input.todos.filter((t) => t.status === 'completed').length;
  const active = input.todos.find((t) => t.status === 'in_progress');
  const summary = active ? active.content : `${done}/${total} done`;

  return (
    <ToolFrame id={id} name="Todo" summary={summary} isError={isError} defaultOpen>
      <ul className="mt-2 space-y-1 text-label">
        {input.todos.map((t, idx) => (
          <li key={idx} className="flex items-start gap-2">
            <span className="mt-0.5 font-mono text-muted-foreground">{todoMarker(t.status)}</span>
            <span className={t.status === 'completed' ? 'text-muted-foreground line-through' : ''}>
              {t.status === 'in_progress' && t.activeForm ? t.activeForm : t.content}
            </span>
          </li>
        ))}
      </ul>
    </ToolFrame>
  );
}
