import React from 'react';
import { Brain } from 'lucide-react';
import { cleanThinkingText } from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';

interface ThinkingBlockProps {
  text: string;
  label?: string;
}

export function ThinkingBlock({ text, label }: ThinkingBlockProps): React.ReactElement {
  const cleaned = cleanThinkingText(text);
  return (
    <div
      data-testid="thinking-block"
      className="my-0.5 flex items-start gap-2 px-2.5 py-1 text-muted-foreground [&_.text-foreground]:text-muted-foreground"
    >
      <Brain size={14} aria-hidden="true" className="mt-1.5 shrink-0 text-text-3" />
      <div className="min-w-0 flex-1">
        {label && <div className="pt-1 text-label text-text-3">{label}</div>}
        <PrMarkdown text={cleaned} />
      </div>
    </div>
  );
}
