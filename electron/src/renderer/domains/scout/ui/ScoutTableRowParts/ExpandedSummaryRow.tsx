import React from 'react';
import Markdown from 'react-markdown';
import { TableRow, TableCell } from '#renderer/global/ui/primitives/table';
import { Collapsible, CollapsibleContent } from '#renderer/global/ui/primitives/collapsible';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

interface ExpandedSummaryRowProps {
  itemId: number;
  isExpanded: boolean;
  summaryLoading: boolean;
  summaryContent: string | null | undefined;
  summaryError: string | undefined;
}

export function ExpandedSummaryRow({
  itemId,
  isExpanded,
  summaryLoading,
  summaryContent,
  summaryError,
}: ExpandedSummaryRowProps): React.ReactElement {
  return (
    <TableRow className="hover:bg-transparent">
      <TableCell colSpan={4} className="p-0">
        <Collapsible open={isExpanded}>
          <CollapsibleContent id={`scout-summary-${itemId}`}>
            {summaryLoading ? (
              <div className="space-y-2 px-10 py-3">
                <Skeleton className="h-4 w-3/4" />
                <Skeleton className="h-4 w-1/2" />
                <Skeleton className="h-4 w-2/3" />
              </div>
            ) : summaryContent ? (
              <div className="prose-scout bg-muted px-10 py-3">
                <Markdown>{summaryContent}</Markdown>
              </div>
            ) : summaryError ? (
              <div className="px-10 py-3 text-xs text-destructive">{summaryError}</div>
            ) : null}
          </CollapsibleContent>
        </Collapsible>
      </TableCell>
    </TableRow>
  );
}
