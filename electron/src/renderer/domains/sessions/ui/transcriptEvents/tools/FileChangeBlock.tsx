import React from 'react';
import type { FileChangeInput } from '#renderer/global/types';
import {
  diffLineTone,
  fileChangeSummary,
  fileChangeVerb,
} from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { ToolFrame } from '#renderer/domains/sessions/ui/transcriptEvents/ToolFrame';

interface FileChangeBlockProps {
  id: string;
  input: FileChangeInput;
}

export function FileChangeBlock({ id, input }: FileChangeBlockProps): React.ReactElement {
  return (
    <ToolFrame id={id} name="Files" summary={fileChangeSummary(input)}>
      <div className="mt-2 space-y-3">
        {input.changes.map((change, changeIndex) => (
          <div
            key={`${change.path}-${changeIndex}`}
            className="overflow-hidden rounded-md bg-muted/40"
          >
            <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2 text-label">
              <span className="font-medium text-foreground">{fileChangeVerb(change)}</span>
              <span className="min-w-0 truncate font-mono text-muted-foreground">
                {change.path}
              </span>
              {change.movePath && (
                <span className="min-w-0 truncate font-mono text-text-3">→ {change.movePath}</span>
              )}
            </div>
            {change.diff && (
              <pre className="max-h-72 overflow-auto px-3 py-2 font-mono text-label leading-5">
                {change.diff.split('\n').map((line, lineIndex) => (
                  <div key={lineIndex} className={diffLineTone(line)}>
                    {line || ' '}
                  </div>
                ))}
              </pre>
            )}
          </div>
        ))}
      </div>
    </ToolFrame>
  );
}
