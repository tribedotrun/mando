import React from 'react';
import { Archive, ArchiveRestore } from 'lucide-react';
import { compactRelativeTime } from '#renderer/global/service/utils';

export function WorkbenchRowHoverActions({
  activity,
  canArchive,
  canUnarchive,
  onArchive,
  onUnarchive,
}: {
  activity: string | null | undefined;
  canArchive: boolean;
  canUnarchive: boolean;
  onArchive: () => void;
  onUnarchive: () => void;
}): React.ReactElement {
  const hideOnHover = canArchive || canUnarchive;
  return (
    <span className="flex shrink-0 items-center gap-1">
      {activity && (
        <span className={`text-[11px] text-text-3 ${hideOnHover ? 'group-hover:hidden' : ''}`}>
          {compactRelativeTime(activity)}
        </span>
      )}
      {canArchive && (
        <span
          role="button"
          tabIndex={-1}
          title="Archive workbench"
          onClick={(e) => {
            e.stopPropagation();
            onArchive();
          }}
          className="hidden shrink-0 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
          style={{ cursor: 'pointer' }}
        >
          <Archive size={11} />
        </span>
      )}
      {canUnarchive && (
        <span
          role="button"
          tabIndex={-1}
          title="Unarchive workbench"
          onClick={(e) => {
            e.stopPropagation();
            onUnarchive();
          }}
          className="hidden shrink-0 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground group-hover:flex"
          style={{ cursor: 'pointer' }}
        >
          <ArchiveRestore size={11} />
        </span>
      )}
    </span>
  );
}
