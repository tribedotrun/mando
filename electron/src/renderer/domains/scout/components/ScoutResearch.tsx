import React, { useState } from 'react';
import { ChevronRight, ExternalLink, Loader2 } from 'lucide-react';
import { useResearchRuns, useResearchRunItems } from '#renderer/hooks/queries';
import { Badge } from '#renderer/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '#renderer/components/ui/table';
import { formatElapsed, relativeTime } from '#renderer/utils';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import type { ScoutResearchRun, ScoutItem } from '#renderer/types';

const ELAPSED_TICK_MS = 1000;

function ElapsedTime({ since }: { since: string }) {
  const [now, setNow] = useState(Date.now);
  useMountEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ELAPSED_TICK_MS);
    return () => clearInterval(id);
  });
  const elapsed = now - new Date(since).getTime();
  if (Number.isNaN(elapsed) || elapsed < 0) return null;
  return <span className="text-text-3 tabular-nums">{formatElapsed(elapsed)}</span>;
}

function statusBadge(status: ScoutResearchRun['status'], createdAt?: string) {
  switch (status) {
    case 'running':
      return (
        <span className="flex items-center gap-2">
          <Badge variant="outline" className="gap-1">
            <Loader2 size={12} className="animate-spin" />
            Running
          </Badge>
          {createdAt && <ElapsedTime since={createdAt} />}
        </span>
      );
    case 'done':
      return <Badge variant="secondary">Done</Badge>;
    case 'failed':
      return <Badge variant="destructive">Failed</Badge>;
  }
}

function RunItems({ runId }: { runId: number }) {
  const { data: items, isLoading } = useResearchRunItems(runId);

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 px-4 py-3 text-text-3 text-caption">
        <Loader2 size={14} className="animate-spin" />
        Loading items...
      </div>
    );
  }

  if (!items?.length) {
    return <div className="px-4 py-3 text-text-3 text-caption">No items discovered</div>;
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
                  className="inline-flex items-center gap-1 text-foreground hover:underline"
                >
                  {item.title || item.url}
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

function ResearchRow({ run }: { run: ScoutResearchRun }) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <TableRow className="group cursor-pointer" onClick={() => setOpen((v) => !v)}>
        <TableCell>
          <span className="flex items-center gap-1.5 text-left">
            <ChevronRight
              size={14}
              className={`shrink-0 text-text-3 transition-transform ${open ? 'rotate-90' : ''}`}
            />
            <span className="line-clamp-1 text-body text-foreground">{run.research_prompt}</span>
          </span>
        </TableCell>
        <TableCell>{statusBadge(run.status, run.created_at)}</TableCell>
        <TableCell className="tabular-nums text-text-3">{run.added_count}</TableCell>
        <TableCell className="text-text-3" title={run.created_at}>
          {relativeTime(run.created_at)}
        </TableCell>
        <TableCell className="text-text-3">
          {run.completed_at ? relativeTime(run.completed_at) : '\u2014'}
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

export function ScoutResearch() {
  const { data: runs, isLoading } = useResearchRuns();

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <h2 className="text-heading text-foreground">Research History</h2>
        {runs && (
          <span className="text-caption text-text-3">
            {runs.length >= 50 ? 'Last 50 runs' : `${runs.length} runs`}
          </span>
        )}
      </div>

      {isLoading ? (
        <div className="flex items-center gap-2 py-8 text-text-3 justify-center">
          <Loader2 size={16} className="animate-spin" />
          Loading...
        </div>
      ) : !runs?.length ? (
        <div className="py-12 text-center text-text-3">
          No research runs yet. Use the Research button to start one.
        </div>
      ) : (
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead>Prompt</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Added</TableHead>
              <TableHead>Started</TableHead>
              <TableHead>Completed</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {runs.map((run) => (
              <ResearchRow key={run.id} run={run} />
            ))}
          </TableBody>
        </Table>
      )}

      {runs?.some((r) => r.status === 'failed' && r.error) && (
        <div className="flex flex-col gap-2">
          {runs
            .filter((r) => r.status === 'failed' && r.error)
            .map((r) => (
              <div
                key={r.id}
                className="rounded-md bg-destructive/10 px-3 py-2 text-caption text-destructive"
              >
                <span className="font-medium">
                  Failed: &ldquo;{r.research_prompt.slice(0, 60)}
                  {r.research_prompt.length > 60 ? '...' : ''}&rdquo;
                </span>
                {' \u2014 '}
                {r.error}
              </div>
            ))}
        </div>
      )}
    </div>
  );
}
