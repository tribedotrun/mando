import React from 'react';
import { Check } from 'lucide-react';
import {
  SCOUT_TYPE_OPTIONS,
  SCOUT_STATE_OPTIONS,
  type ScoutStatusFilter,
} from '#renderer/domains/scout/service/researchHelpers';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';

export function ScoutFilterMenu({
  typeValue,
  stateValue,
  onTypeChange,
  onStateChange,
  open,
  onOpenChange,
  children,
}: {
  typeValue: string;
  stateValue: ScoutStatusFilter;
  onTypeChange: (v: string) => void;
  onStateChange: (v: ScoutStatusFilter) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuLabel>Type</DropdownMenuLabel>
        {SCOUT_TYPE_OPTIONS.map((opt) => {
          const active = typeValue === opt;
          return (
            <DropdownMenuItem
              key={opt}
              onSelect={(e) => {
                e.preventDefault();
                onTypeChange(opt);
              }}
              className={active ? 'text-foreground' : ''}
            >
              <span className="flex-1 capitalize">{opt}</span>
              {active && <Check size={14} />}
            </DropdownMenuItem>
          );
        })}
        <DropdownMenuSeparator />
        <DropdownMenuLabel>State</DropdownMenuLabel>
        {SCOUT_STATE_OPTIONS.map((opt) => {
          const active = stateValue === opt;
          return (
            <DropdownMenuItem
              key={opt}
              onSelect={(e) => {
                e.preventDefault();
                onStateChange(opt);
              }}
              className={active ? 'text-foreground' : ''}
            >
              <span className="flex-1 capitalize">{opt}</span>
              {active && <Check size={14} />}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
