import React from 'react';
import { SlidersHorizontal } from 'lucide-react';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';

export function ViewOptions(): React.ReactElement {
  const { showArchived, setShowArchived } = useTaskStore();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="View options"
          className="text-muted-foreground data-[state=open]:bg-secondary data-[state=open]:text-foreground"
        >
          <SlidersHorizontal size={16} />
        </Button>
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
