import React from 'react';
import Markdown from 'react-markdown';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/global/ui/primitives/collapsible';

interface Props {
  summary: string;
  summaryOpen: boolean;
  onToggle: () => void;
}

export function ScoutSummary({ summary, summaryOpen, onToggle }: Props): React.ReactElement {
  return (
    <Collapsible open={summaryOpen} onOpenChange={onToggle} className="mb-5">
      <CollapsibleTrigger className="flex items-center gap-2 text-label text-muted-foreground">
        <span className="text-[0.6rem]">{summaryOpen ? '\u25BC' : '\u25B6'}</span>
        Process Summary
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div
          className="prose-scout mt-2 border-l-2 pl-4 text-xs leading-relaxed text-foreground"
          style={{
            borderColor: 'var(--border)',
          }}
        >
          <Markdown>{summary}</Markdown>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
