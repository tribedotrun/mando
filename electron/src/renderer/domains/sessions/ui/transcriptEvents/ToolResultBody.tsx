import React from 'react';
import type { UserToolResultBlock } from '#renderer/global/types';
import { extractToolResultText } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

interface ToolResultBodyProps {
  result?: UserToolResultBlock;
  fallbackLang?: string;
}

export function ToolResultBody({
  result,
  fallbackLang = 'text',
}: ToolResultBodyProps): React.ReactNode {
  if (!result) return null;
  const text = extractToolResultText(result);
  if (!text) return null;
  const tone = result.isError ? 'text-destructive' : 'text-muted-foreground';
  return (
    <pre
      className={`mt-2 max-h-60 overflow-auto rounded bg-muted/60 p-2 text-label ${tone}`}
      data-language={fallbackLang}
    >
      {text}
    </pre>
  );
}
