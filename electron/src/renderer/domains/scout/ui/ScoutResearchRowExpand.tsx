import React from 'react';
import { ExternalLink, Loader2, RefreshCw } from 'lucide-react';
import type { ScoutItem, ScoutResearchRun } from '#renderer/global/types';
import { useResearchRunItems, useScoutResearch } from '#renderer/domains/scout/runtime/hooks';
import { Badge } from '#renderer/global/ui/badge';
import { Button } from '#renderer/global/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '#renderer/global/ui/table';

export function ScoutResearchRowExpand({ run }: { run: ScoutResearchRun }): React.ReactElement {
  const research = useScoutResearch();
  const isFailed = run.status === 'failed';

  return (
    <div className="flex flex-col gap-3 border-t border-border/50 bg-muted/20 px-4 py-3">
      <div>
        <div className="text-caption font-medium text-text-3">Prompt</div>
        <div className="mt-1 text-body text-foreground [overflow-wrap:anywhere]">
          {run.research_prompt}
        </div>
      </div>

      {isFailed && run.error && (
        <div className="rounded-md bg-destructive/10 px-3 py-2 text-caption text-destructive [overflow-wrap:anywhere]">
          <span className="font-medium">Error:</span> {run.error}
        </div>
      )}

      {isFailed && (
        <div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={research.isPending}
            onClick={() => research.mutate({ topic: run.research_prompt, process: true })}
          >
            {research.isPending ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <RefreshCw size={14} />
            )}
            Retry
          </Button>
        </div>
      )}

      {!isFailed && <ResearchRunItems runId={run.id} />}
    </div>
  );
}

function ResearchRunItems({ runId }: { runId: number }): React.ReactElement {
  const { data: items, isLoading } = useResearchRunItems(runId);

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-caption text-text-3">
        <Loader2 size={14} className="animate-spin" />
        Loading items...
      </div>
    );
  }

  if (!items?.length) {
    return <div className="text-caption text-text-3">No items discovered</div>;
  }

  return (
    <div className="[&_[data-slot=table-container]]:overflow-x-hidden">
      <Table className="table-fixed">
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="text-caption">Title</TableHead>
            <TableHead className="w-24 text-caption">Status</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {items.map((item: ScoutItem) => (
            <TableRow key={item.id} className="text-caption">
              <TableCell className="max-w-0 truncate">
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
