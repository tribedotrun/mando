import React from 'react';
import type { UserEvent } from '#renderer/global/types';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';

interface UserMessageProps {
  event: UserEvent;
}

export function UserMessage({ event }: UserMessageProps): React.ReactElement | null {
  const texts: string[] = [];
  let imageCount = 0;
  for (const block of event.blocks) {
    if (block.kind === 'text') {
      const t = block.data.text.trim();
      if (t) texts.push(t);
    } else if (block.kind === 'image') {
      imageCount++;
    }
  }
  if (texts.length === 0 && imageCount === 0) return null;
  const body = texts.join('\n').trim();
  if (body.includes('<local-command-caveat>') || body.includes('<local-command-stdout>')) {
    return null;
  }

  return (
    <div className="border-l-2 border-accent/60 bg-muted/30 py-2 pl-3 pr-2">
      {body && (
        <div className="text-sm text-foreground">
          <PrMarkdown text={body} />
        </div>
      )}
      {imageCount > 0 && (
        <p className="mt-1 text-label text-muted-foreground">
          + {imageCount} image{imageCount > 1 ? 's' : ''} attached
        </p>
      )}
    </div>
  );
}
