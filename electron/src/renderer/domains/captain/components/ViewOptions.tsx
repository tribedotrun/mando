import React from 'react';
import { SlidersHorizontal } from 'lucide-react';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
} from '#renderer/global/components/DropdownMenu';

export function ViewOptions(): React.ReactElement {
  const { showArchived, setShowArchived } = useTaskStore();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          aria-label="View options"
          title="View options"
          className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-button border-none bg-transparent text-text-3 transition-colors hover:bg-surface-3 data-[state=open]:bg-surface-3 data-[state=open]:text-text-1"
        >
          <SlidersHorizontal size={16} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[200px]">
        <DropdownMenuLabel>View options</DropdownMenuLabel>
        <DropdownMenuCheckboxItem checked={showArchived} onCheckedChange={setShowArchived}>
          Show archived
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
