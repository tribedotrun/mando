import React, { useMemo, useState } from 'react';
import { Image, Video, ChevronDown, ChevronRight } from 'lucide-react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { summarizeArtifactGroup } from '#renderer/domains/captain/runtime/artifactHelpers';
import { EvidenceMediaList } from '#renderer/domains/captain/ui/ArtifactBlocks/EvidenceMediaList';
import type { TaskArtifact } from '#renderer/global/types';

export function EvidenceBlock({
  artifacts,
  initialExpanded = false,
}: {
  artifacts: TaskArtifact[];
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const { mediaCount, latestTimestamp, hasVideo } = useMemo(
    () => summarizeArtifactGroup(artifacts),
    [artifacts],
  );
  const time = formatEventTime(latestTimestamp);
  const EvidenceIcon = hasVideo ? Video : Image;

  return (
    <div className="mx-3 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <EvidenceIcon size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body font-medium text-text-1">Evidence</span>
            <span className="text-caption text-text-3">
              {mediaCount} {mediaCount === 1 ? 'file' : 'files'}
            </span>
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
        <div className="mt-3">
          <EvidenceMediaList artifacts={artifacts} />
        </div>
      )}
    </div>
  );
}
