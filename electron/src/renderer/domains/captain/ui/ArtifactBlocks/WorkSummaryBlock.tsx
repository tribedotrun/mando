import React, { useState } from 'react';
import { wrapAsciiArt } from '#renderer/domains/captain/service/prHelpers';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import type { TaskArtifact } from '#renderer/global/types';
import { FileText, ChevronDown, ChevronRight } from 'lucide-react';

export function WorkSummaryBlock({
  artifact,
  initialExpanded = false,
}: {
  artifact: TaskArtifact;
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const time = formatEventTime(artifact.created_at);

  return (
    <div className="mx-3 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <FileText size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body font-medium text-text-1">Work Summary</span>
            <span className="text-caption text-text-3">{time}</span>
          </div>
        </div>
        {expanded ? (
          <ChevronDown size={14} className="text-text-3" />
        ) : (
          <ChevronRight size={14} className="text-text-3" />
        )}
      </button>
      {expanded && (
        <div className="mt-3 text-body text-text-1">
          <PrMarkdown text={wrapAsciiArt(artifact.content)} />
        </div>
      )}
    </div>
  );
}
