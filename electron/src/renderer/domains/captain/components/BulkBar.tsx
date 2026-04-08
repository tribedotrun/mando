import React from 'react';
import type { ItemStatus } from '#renderer/types';
import { Button } from '#renderer/components/ui/button';
import { Separator } from '#renderer/components/ui/separator';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/components/ui/select';

const ALL_STATUSES: ItemStatus[] = [
  'new',
  'clarifying',
  'needs-clarification',
  'queued',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
  'awaiting-review',
  'rework',
  'handed-off',
  'escalated',
  'errored',
  'merged',
  'completed-no-pr',
  'canceled',
];

interface Props {
  count: number;
  statuses?: string[];
  onDelete: () => void;
  onBulkStatus?: (status: string) => void;
  onCancel: () => void;
}

export function BulkBar({
  count,
  statuses,
  onDelete,
  onBulkStatus,
  onCancel,
}: Props): React.ReactElement | null {
  if (count === 0) return null;
  const statusList = statuses ?? ALL_STATUSES;

  return (
    <div
      data-testid="bulk-bar"
      className="fixed bottom-12 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-lg bg-muted px-4 py-2 shadow-lg"
    >
      <span className="text-code tabular-nums text-foreground">{count} selected</span>
      {onBulkStatus && (
        <>
          <Separator orientation="vertical" className="h-4" />
          <Select
            value=""
            onValueChange={(value) => {
              if (value) onBulkStatus(value);
            }}
          >
            <SelectTrigger
              size="sm"
              aria-label="Set status for selected items"
              className="bg-secondary text-[12px] text-muted-foreground"
            >
              <SelectValue placeholder="set status..." />
            </SelectTrigger>
            <SelectContent>
              {statusList.map((s) => (
                <SelectItem key={s} value={s}>
                  {s}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </>
      )}
      <Button variant="destructive" size="xs" onClick={onDelete}>
        Delete
      </Button>
      <Button variant="outline" size="xs" onClick={onCancel}>
        Clear
      </Button>
    </div>
  );
}
