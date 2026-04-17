import React, { useState } from 'react';
import { ChevronRight, ExternalLink, Loader2 } from 'lucide-react';
import type { ScoutItem, ScoutResearchRun } from '#renderer/global/types';
import { useNow } from '#renderer/domains/scout/runtime/useNow';
import { useResearchRunItems } from '#renderer/domains/scout/runtime/hooks';
import { formatElapsed, relativeTime } from '#renderer/global/service/utils';
import { statusBadgeConfig } from '#renderer/domains/scout/service/researchHelpers';
import { Badge } from '#renderer/global/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '#renderer/global/ui/table';

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

function RunItems({ runId }: { runId: number }): React.ReactElement {
  const { data: items, isLoading } = useResearchRunItems(runId);

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 px-4 py-3 text-caption text-text-3">
        <Loader2 size={14} className="animate-spin" />
        Loading items...
      </div>
    );
  }

  if (!items?.length) {
    return <div className="px-4 py-3 text-caption text-text-3">No items discovered</div>;
  }

  return (
    <div className="border-t border-border/50">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="text-caption">Title</TableHead>
            <TableHead className="text-caption">Status</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {items.map((item: ScoutItem) => (
            <TableRow key={item.id} className="text-caption">
              <TableCell className="max-w-[400px] truncate">
                <a
                  href={item.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex min-w-0 max-w-full items-center gap-1 text-foreground hover:underline"
                  title={item.title || item.url}
                >
                  <span className="truncate">{item.title || item.url}</span>
                  <ExternalLink size={11} className="shrink-0 text-text-3" />
                </a>
              </TableCell>
              <TableCell>
                <Badge variant="outline" className="text-[11px]">
                  {item.status}
                </Badge>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

export function ResearchRow({ run }: { run: ScoutResearchRun }): React.ReactElement {
  const [open, setOpen] = useState(false);

  return (
    <>
      <TableRow className="group cursor-pointer" onClick={() => setOpen((value) => !value)}>
        <TableCell>
          <span className="flex items-center gap-1.5 text-left">
            <ChevronRight
              size={14}
              className={`shrink-0 text-text-3 transition-transform ${open ? 'rotate-90' : ''}`}
            />
            <span className="line-clamp-1 text-body text-foreground">{run.research_prompt}</span>
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
            <RunItems runId={run.id} />
          </td>
        </tr>
      )}
    </>
  );
}
