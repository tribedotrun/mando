import React, { useMemo } from 'react';
import {
  PromptGroupRow,
  StructuredOutputBlock,
} from '#renderer/domains/sessions/ui/TranscriptBlocks';
import {
  parseTranscript,
  filterVisibleSections,
  hoistStructuredOutputs,
  groupSections,
} from '#renderer/domains/sessions/service/transcript';

interface Props {
  markdown: string;
}

export function TranscriptViewer({ markdown }: Props): React.ReactElement {
  const sections = useMemo(() => parseTranscript(markdown), [markdown]);

  const visibleSections = filterVisibleSections(sections);

  if (visibleSections.length === 0) {
    return <div className="text-[11px] text-muted-foreground">No transcript content</div>;
  }

  const hoisted = hoistStructuredOutputs(visibleSections);

  const groups = groupSections(visibleSections);

  return (
    <div className="min-w-0 divide-y divide-border [overflow-wrap:anywhere]">
      {hoisted.map((block, i) => (
        <StructuredOutputBlock key={`hoist-${i}`} block={block} />
      ))}
      {groups.map((group, gi) => (
        <PromptGroupRow key={gi} group={group} />
      ))}
    </div>
  );
}
