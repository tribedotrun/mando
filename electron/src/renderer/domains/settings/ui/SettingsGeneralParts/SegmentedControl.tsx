import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function SegmentedControl({
  options,
  value,
  onChange,
  disabled,
}: {
  options: readonly string[];
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}): React.ReactElement {
  return (
    <div data-testid="update-channel-select" className="flex overflow-hidden rounded-md bg-muted">
      {options.map((opt) => {
        const active = value === opt;
        return (
          <Button
            key={opt}
            variant="ghost"
            size="sm"
            disabled={disabled}
            onClick={() => onChange(opt)}
            className={`h-auto rounded-none px-4 py-1 text-[13px] transition-colors ${
              active
                ? 'bg-secondary font-medium text-foreground'
                : 'bg-transparent font-normal text-muted-foreground'
            }`}
          >
            {opt.charAt(0).toUpperCase() + opt.slice(1)}
          </Button>
        );
      })}
    </div>
  );
}
