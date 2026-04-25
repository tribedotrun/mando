import React from 'react';
import { Switch } from '#renderer/global/ui/primitives/switch';

interface TaskPlanModeToggleProps {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  className?: string;
}

export function TaskPlanModeToggle({
  checked,
  onCheckedChange,
  className,
}: TaskPlanModeToggleProps): React.ReactElement {
  return (
    <label className={className ?? 'flex items-center gap-1.5 text-[12px] text-muted-foreground'}>
      <Switch checked={checked} onCheckedChange={onCheckedChange} className="scale-75" />
      Plan mode
    </label>
  );
}
