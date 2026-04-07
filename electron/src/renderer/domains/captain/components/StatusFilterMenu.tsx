import React from 'react';
import { Check } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '#renderer/global/components/DropdownMenu';

const STATUS_OPTIONS = ['all', 'running', 'stopped', 'failed'] as const;

export function StatusFilterMenu({
  value,
  onChange,
  open,
  onOpenChange,
  children,
}: {
  value: string;
  onChange: (v: string) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Status</DropdownMenuLabel>
        {STATUS_OPTIONS.map((opt) => {
          const active = value === opt;
          return (
            <DropdownMenuItem
              key={opt}
              onSelect={() => onChange(opt)}
              className={active ? 'text-text-1' : ''}
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
