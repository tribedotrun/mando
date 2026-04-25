import React, { useState } from 'react';
import { ChevronRight, Loader2 } from 'lucide-react';
import type { ScoutResearchRun } from '#renderer/global/types';
import { useNow } from '#renderer/domains/scout/runtime/useNow';
import { formatElapsed, relativeTime } from '#renderer/global/service/utils';
import { statusBadgeConfig } from '#renderer/domains/scout/service/researchHelpers';
import { Badge } from '#renderer/global/ui/primitives/badge';
import { TableCell, TableRow } from '#renderer/global/ui/primitives/table';
import { ScoutResearchRowExpand } from '#renderer/domains/scout/ui/ScoutResearchRowExpand';

function ElapsedTime({ since }: { since: string }): React.ReactElement | null {
  const now = useNow();
  const elapsed = now - new Date(since).getTime();
  if (Number.isNaN(elapsed) || elapsed < 0) return null;
  return <span className="text-text-3 tabular-nums">{formatElapsed(elapsed)}</span>;
}

function StatusBadge({
  status,
  createdAt,
}: {
  status: ScoutResearchRun['status'];
  createdAt?: string;
}): React.ReactElement {
  const cfg = statusBadgeConfig(status);
  if (cfg.spinning) {
    return (
      <span className="flex items-center gap-2">
        <Badge variant={cfg.variant} className="gap-1">
          <Loader2 size={12} className="animate-spin" />
          {cfg.label}
        </Badge>
        {cfg.showElapsed && createdAt && <ElapsedTime since={createdAt} />}
      </span>
    );
  }

  return <Badge variant={cfg.variant}>{cfg.label}</Badge>;
}

export function ScoutResearchRow({ run }: { run: ScoutResearchRun }): React.ReactElement {
  const [open, setOpen] = useState(false);

  return (
    <>
      <TableRow className="group cursor-pointer" onClick={() => setOpen((value) => !value)}>
        <TableCell className="max-w-0 truncate">
          <span className="flex min-w-0 items-center gap-1.5 text-left">
            <ChevronRight
              size={14}
              className={`shrink-0 text-text-3 transition-transform ${open ? 'rotate-90' : ''}`}
            />
            <span
              className="min-w-0 flex-1 truncate text-body text-foreground"
              title={run.research_prompt}
            >
              {run.research_prompt}
            </span>
          </span>
        </TableCell>
        <TableCell>
          <StatusBadge status={run.status} createdAt={run.created_at} />
        </TableCell>
        <TableCell className="tabular-nums text-text-3">{run.added_count}</TableCell>
        <TableCell className="text-text-3" title={run.created_at}>
          {relativeTime(run.created_at)}
        </TableCell>
        <TableCell className="text-text-3">
          {run.completed_at ? relativeTime(run.completed_at) : '—'}
        </TableCell>
      </TableRow>
      {open && (
        <tr>
          <td colSpan={5} className="p-0">
            <ScoutResearchRowExpand run={run} />
          </td>
        </tr>
      )}
    </>
  );
}
