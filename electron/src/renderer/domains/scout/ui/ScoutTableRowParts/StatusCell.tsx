import React from 'react';
import { Badge } from '#renderer/global/ui/primitives/badge';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '#renderer/global/ui/primitives/select';
import {
  isUserSettableScoutStatus,
  USER_SETTABLE_STATUSES,
  type ScoutUserSettableStatus,
} from '#renderer/domains/scout/service/researchHelpers';
import type { ScoutItemStatus } from '#renderer/global/types';

interface StatusCellProps {
  itemId: number;
  status: ScoutItemStatus;
  isEditing: boolean;
  statusVariant: 'default' | 'secondary' | 'destructive' | 'outline';
  onStatusChange: (id: number, status: ScoutUserSettableStatus) => void;
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

  if (isEditing && isUserSettableScoutStatus(status)) {
    return (
      <Select
        value={status}
        onValueChange={(value) => {
          if (isUserSettableScoutStatus(value)) onStatusChange(itemId, value);
        }}
        onOpenChange={(open) => {
          if (!open) onStartEdit(-1);
        }}
        open
      >
        <SelectTrigger size="sm" className="h-6 text-[11px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {USER_SETTABLE_STATUSES.map((s) => (
            <SelectItem key={s} value={s}>
              {s}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }

  if (isUserSettableScoutStatus(status)) {
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
