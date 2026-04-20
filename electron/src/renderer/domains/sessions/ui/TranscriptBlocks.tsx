import React from 'react';
import { z } from 'zod';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { CodeBlock } from '#renderer/global/ui/code-block';
import {
  type TranscriptSection,
  type ContentBlock,
  type PromptGroup,
} from '#renderer/domains/sessions/service/transcript';

export type { TranscriptSection, ContentBlock, PromptGroup };
export {
  SectionRow,
  HumanSection,
  TurnContent,
} from '#renderer/domains/sessions/ui/TranscriptBlocksParts';
export { PromptGroupRow } from '#renderer/domains/sessions/ui/PromptGroupRow';

const structuredOutputSchema = z.record(z.string(), z.unknown());

export function StructuredOutputBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const parsed = (() => {
    try {
      const raw: unknown = JSON.parse(block.body);
      const result = structuredOutputSchema.safeParse(raw);
      return result.success ? result.data : null;
    } catch {
      return null;
    }
  })();

  if (!parsed) {
    return (
      <div className="my-2 rounded-md bg-muted px-3 py-2">
        <div className="mb-1 text-label font-medium text-muted-foreground">Structured Output</div>
        <CodeBlock code={block.body} language="json" className="my-0" />
      </div>
    );
  }

  return (
    <div className="my-2 min-w-0 space-y-2 overflow-hidden rounded-md bg-muted px-3 py-2">
      <div className="text-label font-medium text-muted-foreground">Structured Output</div>
      {Object.entries(parsed).map(([key, value]) => {
        const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
        return (
          <div key={key} className="min-w-0">
            <div className="text-label mb-0.5 font-semibold text-muted-foreground">{key}</div>
            <div className="min-w-0 text-caption leading-relaxed text-foreground [overflow-wrap:anywhere]">
              <PrMarkdown text={text} />
            </div>
          </div>
        );
      })}
    </div>
  );
}
