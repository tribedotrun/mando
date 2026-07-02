import React, { useId } from 'react';
import { Switch } from '#renderer/global/ui/primitives/switch';

interface TaskCreateOptionSwitchRowProps {
  label: string;
  description?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

export function TaskCreateOptionSwitchRow({
  label,
  description,
  checked,
  onCheckedChange,
}: TaskCreateOptionSwitchRowProps): React.ReactElement {
  const id = useId();

  return (
    <div className="flex items-center justify-between gap-3 rounded-sm px-2 py-1.5 outline-hidden transition-colors hover:bg-accent hover:text-accent-foreground focus-within:bg-accent focus-within:text-accent-foreground">
      <label htmlFor={id} className="min-w-0 flex-1 cursor-pointer select-none">
        <span className="block text-sm">{label}</span>
        {description && (
          <span className="mt-0.5 block text-caption text-muted-foreground">{description}</span>
        )}
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
