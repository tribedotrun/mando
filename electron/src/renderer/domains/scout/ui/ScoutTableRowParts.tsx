import React from 'react';
import Markdown from 'react-markdown';
import { TableRow, TableCell } from '#renderer/global/ui/table';
import { Collapsible, CollapsibleContent } from '#renderer/global/ui/collapsible';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { Badge } from '#renderer/global/ui/badge';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/global/ui/select';
import { USER_SETTABLE_STATUSES } from '#renderer/domains/scout/service/researchHelpers';

const USER_SETTABLE = USER_SETTABLE_STATUSES as readonly string[];

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

interface StatusCellProps {
  itemId: number;
  status: string;
  isEditing: boolean;
  statusVariant: 'default' | 'secondary' | 'destructive' | 'outline';
  onStatusChange: (id: number, status: string) => void;
  onStartEdit: (id: number) => void;
}

export function StatusCell({
  itemId,
  status,
  isEditing,
  statusVariant,
  onStatusChange,
  onStartEdit,
}: StatusCellProps): React.ReactElement | null {
  if (status === 'processed') return null;

  if (isEditing && USER_SETTABLE.includes(status)) {
    return (
      <Select
        value={status}
        onValueChange={(v) => onStatusChange(itemId, v)}
        onOpenChange={(open) => {
          if (!open) onStartEdit(-1);
        }}
        open
      >
        <SelectTrigger size="sm" className="h-6 text-[11px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {USER_SETTABLE.map((s) => (
            <SelectItem key={s} value={s}>
              {s}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }

  if (USER_SETTABLE.includes(status)) {
    return (
      <button
        type="button"
        onClick={() => onStartEdit(itemId)}
        className="rounded"
        aria-label={`Change status, currently ${status}`}
      >
        <Badge variant={statusVariant} className="cursor-pointer text-[11px]">
          {status}
        </Badge>
      </button>
    );
  }

  return (
    <Badge variant={statusVariant} className="text-[11px]">
      {status}
    </Badge>
  );
}
