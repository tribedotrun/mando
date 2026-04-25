import React from 'react';
import type { AssistantEvent, UserToolResultBlock } from '#renderer/global/types';
import { groupAssistantBlocks } from '#renderer/domains/sessions/service/transcriptEvents';
import { ToolCallBlock } from '#renderer/domains/sessions/ui/transcriptEvents/ToolCallBlock';
import { ToolGroupBlock } from '#renderer/domains/sessions/ui/transcriptEvents/ToolGroupBlock';
import { ThinkingBlock } from '#renderer/domains/sessions/ui/transcriptEvents/ThinkingBlock';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';

interface AssistantMessageProps {
  event: AssistantEvent;
  eventIndex: number;
  toolResults: Map<string, UserToolResultBlock>;
}

export function AssistantMessage({
  event,
  eventIndex,
  toolResults,
}: AssistantMessageProps): React.ReactElement {
  const items = groupAssistantBlocks(event, eventIndex);
  return (
    <div className="py-2">
      {event.model && (
        <div className="mb-1 text-label uppercase tracking-wide text-muted-foreground">
          {event.model}
        </div>
      )}
      <div className="space-y-1">
        {items.map((item) => {
          if (item.kind === 'group') {
            return (
              <ToolGroupBlock
                key={item.group.id}
                id={item.group.id}
                tools={item.group.tools}
                results={toolResults}
              />
            );
          }
          const block = item.block;
          if (block.kind === 'text') {
            const trimmed = block.data.text.trim();
            if (!trimmed) return null;
            return (
              <div
                key={`${item.eventIndex}-${item.blockIndex}`}
                className="text-sm text-foreground"
              >
                <PrMarkdown text={trimmed} />
              </div>
            );
          }
          if (block.kind === 'thinking') {
            return (
              <ThinkingBlock
                key={`${item.eventIndex}-${item.blockIndex}`}
                id={`${item.eventIndex}-${item.blockIndex}`}
                text={block.data.text}
              />
            );
          }
          return (
            <ToolCallBlock
              key={block.data.id}
              toolUse={block.data}
              result={toolResults.get(block.data.id)}
            />
          );
        })}
      </div>
    </div>
  );
}
