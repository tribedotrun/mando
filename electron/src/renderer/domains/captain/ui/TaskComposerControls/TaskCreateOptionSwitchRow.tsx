import React, { useId } from 'react';
import { Switch } from '#renderer/global/ui/primitives/switch';

interface TaskCreateOptionSwitchRowProps {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

export function TaskCreateOptionSwitchRow({
  label,
  checked,
  onCheckedChange,
}: TaskCreateOptionSwitchRowProps): React.ReactElement {
  const id = useId();

  return (
    <div className="flex min-h-8 items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-sm outline-hidden transition-colors hover:bg-accent hover:text-accent-foreground focus-within:bg-accent focus-within:text-accent-foreground">
      <label htmlFor={id} className="flex-1 cursor-pointer select-none">
        {label}
      </label>
      <Switch
        id={id}
        size="sm"
        checked={checked}
        onCheckedChange={onCheckedChange}
        aria-label={label}
      />
    </div>
  );
}
