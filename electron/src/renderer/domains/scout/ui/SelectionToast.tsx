import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Separator } from '#renderer/global/ui/primitives/separator';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/global/ui/primitives/select';

interface Props<TStatus extends string> {
  count: number;
  statuses: readonly TStatus[];
  onDelete: () => void;
  onBulkStatus: (status: TStatus) => void;
  onCancel: () => void;
}

export function SelectionToast<TStatus extends string>({
  count,
  statuses,
  onDelete,
  onBulkStatus,
  onCancel,
}: Props<TStatus>): React.ReactElement | null {
  if (count === 0) return null;

  return (
    <div
      data-testid="selection-toast"
      className="fixed bottom-12 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-lg bg-muted px-4 py-2 shadow-lg"
    >
      <span className="text-code tabular-nums text-foreground">{count} selected</span>
      <>
        <Separator orientation="vertical" className="h-4" />
        <Select
          value=""
          onValueChange={(value) => {
            const selectedStatus = statuses.find((status) => status === value);
            if (selectedStatus) onBulkStatus(selectedStatus);
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
            {statuses.map((s) => (
              <SelectItem key={s} value={s}>
                {s}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </>
      <Button variant="destructive" size="xs" onClick={onDelete}>
        Delete
      </Button>
      <Button variant="outline" size="xs" onClick={onCancel}>
        Clear
      </Button>
    </div>
  );
}
