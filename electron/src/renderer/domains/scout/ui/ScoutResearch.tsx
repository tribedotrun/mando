import React from 'react';
import { Loader2 } from 'lucide-react';
import { useResearchRuns } from '#renderer/domains/scout/runtime/hooks';
import { Table, TableBody, TableHead, TableHeader, TableRow } from '#renderer/global/ui/table';
import { ResearchRow } from '#renderer/domains/scout/ui/ScoutResearchRow';

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
        <div className="flex items-center justify-center gap-2 py-8 text-text-3">
          <Loader2 size={16} className="animate-spin" />
          Loading...
        </div>
      ) : !runs?.length ? (
        <div className="py-12 text-center text-text-3">
          No research runs yet. Use the Research button to start one.
        </div>
      ) : (
        <Table className="table-fixed">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead>Prompt</TableHead>
              <TableHead className="w-32">Status</TableHead>
              <TableHead className="w-20">Added</TableHead>
              <TableHead className="w-28">Started</TableHead>
              <TableHead className="w-28">Completed</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {runs.map((run) => (
              <ResearchRow key={run.id} run={run} />
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}
