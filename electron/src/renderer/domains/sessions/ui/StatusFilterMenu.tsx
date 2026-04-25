import React from 'react';
import { Check } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';
import type { SessionStatusFilter } from '#renderer/domains/sessions/service/helpers';

export function StatusFilterMenu({
  value,
  onChange,
  options,
  open,
  onOpenChange,
  children,
}: {
  value: SessionStatusFilter;
  onChange: (v: SessionStatusFilter) => void;
  options: readonly SessionStatusFilter[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Status</DropdownMenuLabel>
        {options.map((opt) => {
          const active = value === opt;
          return (
            <DropdownMenuItem
              key={opt}
              onSelect={() => onChange(opt)}
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
