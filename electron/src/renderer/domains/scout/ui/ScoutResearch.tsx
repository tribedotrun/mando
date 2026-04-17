import React from 'react';
import { Loader2 } from 'lucide-react';
import { useResearchRuns } from '#renderer/domains/scout/runtime/hooks';
import { Table, TableBody, TableHead, TableHeader, TableRow } from '#renderer/global/ui/table';
import { failedRunsWithErrors } from '#renderer/domains/scout/service/researchHelpers';
import { ResearchRow } from '#renderer/domains/scout/ui/ScoutResearchRow';

export function ScoutResearch() {
  const { data: runs, isLoading } = useResearchRuns();
  const failedRuns = failedRunsWithErrors(runs ?? []);

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

      {failedRuns.length > 0 && (
        <div className="flex flex-col gap-2">
          {failedRuns.map((r) => (
            <div
              key={r.id}
              className="rounded-md bg-destructive/10 px-3 py-2 text-caption text-destructive [overflow-wrap:anywhere]"
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
